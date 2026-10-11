//! Formulaires `multipart/form-data` : champs texte et fichiers, lus au fil
//! de l'eau, avec la limite de taille du corps.
#![cfg(feature = "multipart")]

use vitesse::prelude::*;
use vitesse::test::TestClient;

const BOUNDARY: &str = "----vitesse-boundary";

fn form(parts: &[(&str, Option<&str>, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, file, content) in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        match file {
            Some(file) => body.extend_from_slice(
                format!(
                    "content-disposition: form-data; name=\"{name}\"; filename=\"{file}\"\r\ncontent-type: text/plain\r\n\r\n"
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("content-disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn summary(req: Request) -> vitesse::Result<String> {
    let mut form = req.multipart()?;
    let mut out = Vec::new();
    while let Some(field) = form.next_field().await? {
        let name = field.name().unwrap_or("?").to_owned();
        match field.file_name().map(str::to_owned) {
            Some(file) => {
                let ty = field.content_type().unwrap_or("-").to_owned();
                let bytes = field.bytes().await?;
                out.push(format!("{name}:{file}:{ty}:{}", bytes.len()));
            }
            None => out.push(format!("{name}={}", field.text().await?)),
        }
    }
    Ok(out.join(" "))
}

fn client() -> TestClient {
    let mut app = App::new();
    app.post("/upload", summary);
    app.post(
        "/big",
        summary.with(middleware::body_limit(4 * 1024 * 1024)),
    );
    let dir = std::env::temp_dir().join(format!("vitesse-multipart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    app.state(dir);
    app.post("/save", |req: Request| async move {
        let dir = req.state::<std::path::PathBuf>();
        let mut form = req.multipart()?;
        let field = form
            .next_field()
            .await?
            .ok_or(Error::bad_request("no file"))?;
        let path = dir.join("upload.bin");
        let size = field.save(&path).await?;
        let written = tokio::fs::read(&path).await?;
        Ok::<_, Error>(format!("{size} {}", written.len()))
    });
    TestClient::new(app)
}

fn content_type() -> String {
    format!("multipart/form-data; boundary={BOUNDARY}")
}

#[tokio::test]
async fn fields_and_files() {
    let c = client();
    let body = form(&[
        ("title", None, "Vacances à Biarritz".as_bytes()),
        ("photo", Some("plage.txt"), &[b'x'; 5000]),
    ]);
    let r = c
        .post("/upload")
        .header("content-type", &content_type())
        .body(body)
        .await;
    assert_eq!(r.status(), 200, "{}", r.text());
    assert_eq!(
        r.text(),
        "title=Vacances à Biarritz photo:plage.txt:text/plain:5000"
    );
}

#[tokio::test]
async fn files_can_be_streamed_to_disk() {
    let c = client();
    let body = form(&[("file", Some("a.bin"), &[7u8; 100_000])]);
    let r = c
        .post("/save")
        .header("content-type", &content_type())
        .body(body)
        .await;
    assert_eq!(r.text(), "100000 100000");
}

#[tokio::test]
async fn limits_and_errors() {
    let c = client();
    // 2 Mo : au-delà de la limite par défaut (1 Mio), pas de celle de /big.
    let body = form(&[("file", Some("big.bin"), &vec![1u8; 2 * 1024 * 1024])]);
    let r = c
        .post("/upload")
        .header("content-type", &content_type())
        .body(body.clone())
        .await;
    assert_eq!(r.status(), 413);
    let r = c
        .post("/big")
        .header("content-type", &content_type())
        .body(body)
        .await;
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.text(),
        format!("file:big.bin:text/plain:{}", 2 * 1024 * 1024)
    );

    let r = c.post("/upload").json(&json!({ "not": "multipart" })).await;
    assert_eq!(r.status(), 415);
    let r = c
        .post("/upload")
        .header("content-type", &content_type())
        .body("garbage")
        .await;
    assert_eq!(r.status(), 400);
}

#[tokio::test]
async fn a_failed_save_leaves_no_partial_file() {
    let dir = std::env::temp_dir().join(format!("vitesse-partial-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("upload.bin");
    let mut app = App::new();
    app.body_limit(10_000);
    app.state(path.clone());
    app.post("/save", |req: Request| async move {
        let path = req.state::<std::path::PathBuf>();
        let mut form = req.multipart()?;
        let field = form
            .next_field()
            .await?
            .ok_or(Error::bad_request("no file"))?;
        field.save(path).await.map(|size| size.to_string())
    });
    let body = form(&[("file", Some("big.bin"), &[1u8; 50_000])]);
    let r = TestClient::new(app)
        .post("/save")
        .header("content-type", &content_type())
        .body(body)
        .await;
    assert_eq!(r.status(), 413);
    assert!(!path.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
