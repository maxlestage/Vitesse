//! `middleware::compress()`: Brotli or gzip for the browsers that accept them, and only
//! for the bodies worth it.
#![cfg(feature = "compress")]

use std::io::Read;

use vitesse::prelude::*;
use vitesse::test::TestClient;

fn app() -> App {
    let mut app = App::new();
    app.middleware(middleware::compress());
    app.get("/page", |_| async { Html("<p>hello</p>".repeat(200)) })
        .get("/small", |_| async { Html("<p>hi</p>") })
        .get("/image", |_| async {
            res::send(vec![0u8; 4096]).header("content-type", "image/png")
        })
        .get("/encoded", |_| async {
            res::text("x".repeat(4096)).header("content-encoding", "identity")
        })
        .get("/varies", |_| async {
            Html("<p>hello</p>".repeat(200))
                .into_response()
                .header("vary", "Cookie")
        });
    app
}

#[tokio::test]
async fn compresses_text_for_browsers_that_accept_it() {
    let client = TestClient::new(app());
    let res = client
        .get("/page")
        .header("accept-encoding", "gzip, deflate, br")
        .await;
    assert_eq!(res.status(), 200);
    assert_eq!(res.header("content-encoding"), Some("br"));
    assert_eq!(res.header("vary"), Some("accept-encoding"));
    assert_eq!(res.header("content-type"), Some("text/html; charset=utf-8"));
    let mut decoded = Vec::new();
    brotli::Decompressor::new(res.bytes().as_ref(), 4096)
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, "<p>hello</p>".repeat(200).as_bytes());

    let res = client.get("/page").header("accept-encoding", "gzip").await;
    assert_eq!(res.header("content-encoding"), Some("gzip"));
    let mut decoded = String::new();
    flate2::read::GzDecoder::new(res.bytes().as_ref())
        .read_to_string(&mut decoded)
        .unwrap();
    assert_eq!(decoded, "<p>hello</p>".repeat(200));

    let res = client.get("/varies").header("accept-encoding", "br").await;
    assert_eq!(res.header("vary"), Some("Cookie, accept-encoding"));
}

#[tokio::test]
async fn leaves_the_rest_alone() {
    let client = TestClient::new(app());
    // Browsers that do not ask for it, or refuse it.
    assert_eq!(client.get("/page").await.header("content-encoding"), None);
    let refused = client
        .get("/page")
        .header("accept-encoding", "br;q=0, gzip;q=0")
        .await;
    assert_eq!(refused.header("content-encoding"), None);
    assert_eq!(refused.text(), "<p>hello</p>".repeat(200));
    // Small bodies, images and bodies encoded already.
    for path in ["/small", "/image", "/encoded"] {
        let res = client.get(path).header("accept-encoding", "br, gzip").await;
        assert_ne!(res.header("content-encoding"), Some("br"), "{path}");
        assert_ne!(res.header("content-encoding"), Some("gzip"), "{path}");
    }
}

#[tokio::test]
async fn decompresses_request_bodies() {
    use std::io::Write;

    let mut app = App::new();
    app.middleware(middleware::decompress());
    app.body_limit(10_000);
    app.post("/echo", |req: Request| async move {
        let text = req.text().await?;
        Ok::<_, Error>(format!(
            "{} {}",
            req.header("content-encoding").unwrap_or("-"),
            text.len()
        ))
    });
    let client = TestClient::new(app);
    let text = "bonjour ".repeat(1000);

    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(text.as_bytes()).unwrap();
    let res = client
        .post("/echo")
        .header("content-encoding", "gzip")
        .body(gzip.finish().unwrap())
        .await;
    assert_eq!(res.text(), "- 8000");

    let mut br = Vec::new();
    {
        let mut w = brotli::CompressorWriter::new(&mut br, 4096, 5, 22);
        w.write_all(text.as_bytes()).unwrap();
    }
    let res = client
        .post("/echo")
        .header("content-encoding", "br")
        .body(br)
        .await;
    assert_eq!(res.text(), "- 8000");

    let mut deflate = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    deflate.write_all(text.as_bytes()).unwrap();
    let res = client
        .post("/echo")
        .header("content-encoding", "deflate")
        .body(deflate.finish().unwrap())
        .await;
    assert_eq!(res.text(), "- 8000");

    // Petit une fois compressé, trop gros une fois décompressé.
    let mut bomb = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    bomb.write_all(&vec![0u8; 1_000_000]).unwrap();
    let bomb = bomb.finish().unwrap();
    assert!(bomb.len() < 10_000);
    let res = client
        .post("/echo")
        .header("content-encoding", "gzip")
        .body(bomb)
        .await;
    assert_eq!(res.status(), 413);

    let res = client
        .post("/echo")
        .header("content-encoding", "zstd")
        .body("x")
        .await;
    assert_eq!(res.status(), 415);
    let res = client
        .post("/echo")
        .header("content-encoding", "gzip")
        .body("not gzip")
        .await;
    assert_eq!(res.status(), 400);
    // Sans `Content-Encoding`, rien ne change.
    assert_eq!(client.post("/echo").body("abc").await.text(), "- 3");
}

/// A static file is compressed once at the best quality, for every request
/// after; changed on disk, it is read and compressed again.
#[tokio::test]
async fn static_files_are_compressed_once_and_follow_the_disk() {
    let dir = std::env::temp_dir().join(format!("vitesse-compress-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let css = |n: usize| {
        (0..n)
            .map(|i| format!(".c{i} {{ color: #{:06x}; }}\n", i * 977))
            .collect::<String>()
    };
    let first = css(400);
    std::fs::write(dir.join("site.css"), &first).unwrap();
    let mut app = App::new();
    app.middleware(middleware::compress());
    app.static_dir("/static", &dir);
    let client = TestClient::new(app);

    let brotli = |bytes: &[u8]| {
        let mut out = Vec::new();
        brotli::Decompressor::new(bytes, 4096)
            .read_to_end(&mut out)
            .unwrap();
        String::from_utf8(out).unwrap()
    };
    let mut sizes = Vec::new();
    for _ in 0..3 {
        let res = client
            .get("/static/site.css")
            .header("accept-encoding", "br")
            .await;
        assert_eq!(res.status(), 200);
        assert_eq!(res.header("content-encoding"), Some("br"));
        assert_eq!(res.header("vary"), Some("accept-encoding"));
        assert_eq!(brotli(res.bytes().as_ref()), first);
        sizes.push(res.bytes().len());
    }
    // The same compressed bytes each time, smaller than a fast compression.
    assert!(sizes.windows(2).all(|w| w[0] == w[1]), "{sizes:?}");
    let mut fast = brotli::CompressorWriter::new(Vec::new(), 4096, 1, 22);
    std::io::Write::write_all(&mut fast, first.as_bytes()).unwrap();
    assert!(sizes[0] < fast.into_inner().len());

    let res = client
        .get("/static/site.css")
        .header("accept-encoding", "gzip")
        .await;
    assert_eq!(res.header("content-encoding"), Some("gzip"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(res.bytes().as_ref())
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, first);
    // Without compression, the file as it is; and a range is never compressed.
    assert_eq!(client.get("/static/site.css").await.text(), first);
    let res = client
        .get("/static/site.css")
        .header("accept-encoding", "br")
        .header("range", "bytes=0-99")
        .await;
    assert_eq!(res.status(), 206);
    assert_eq!(res.text(), first[..100]);

    // Changed on disk (another size, and another date): read and compressed again.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let second = css(500);
    std::fs::write(dir.join("site.css"), &second).unwrap();
    let res = client
        .get("/static/site.css")
        .header("accept-encoding", "br")
        .await;
    assert_eq!(brotli(res.bytes().as_ref()), second);
    assert_eq!(client.get("/static/site.css").await.text(), second);
    let _ = std::fs::remove_dir_all(&dir);
}
