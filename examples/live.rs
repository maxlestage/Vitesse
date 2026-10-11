//! Server-Sent Events and file uploads: a page that receives the server's
//! clock live (`EventSource`) and sends files with a multipart form.
//!
//! ```sh
//! cargo run --example live --features multipart
//! # then open http://localhost:3000
//! ```

use std::time::Duration;

use vitesse::prelude::*;
use vitesse::sse::Event;

const PAGE: &str = r#"<!doctype html>
<meta charset="utf-8">
<title>Vitesse live</title>
<h1>Clock: <span id="clock">…</span></h1>
<form method="post" action="/upload" enctype="multipart/form-data">
  <input name="title" placeholder="Title">
  <input type="file" name="files" multiple>
  <button>Upload</button>
</form>
<script>
  new EventSource("/clock").addEventListener("tick", (e) => {
    document.getElementById("clock").textContent = JSON.parse(e.data).time;
  });
</script>
"#;

async fn upload(req: Request) -> vitesse::Result<Html<String>> {
    let mut form = req.multipart()?;
    let mut lines = Vec::new();
    while let Some(field) = form.next_field().await? {
        let name = field.name().unwrap_or("?").to_owned();
        match field.file_name().map(str::to_owned) {
            Some(file) => {
                let size = field.bytes().await?.len();
                lines.push(format!("<li>{name}: {file} ({size} bytes)</li>"));
            }
            None => lines.push(format!("<li>{name} = {}</li>", field.text().await?)),
        }
    }
    Ok(Html(format!(
        "<ul>{}</ul><a href=\"/\">Back</a>",
        lines.join("")
    )))
}

fn main() -> std::io::Result<()> {
    let mut app = App::new();
    app.middleware(middleware::logger());
    app.get("/", |_| async { Html(PAGE) });
    app.get("/clock", |_| async {
        let (tx, events) = sse::channel(8);
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            loop {
                let time = format!("{:.0} s", start.elapsed().as_secs_f64());
                let event = Event::new()
                    .event("tick")
                    .json_data(&json!({ "time": time }));
                // `send` fails once the browser has closed the page.
                if tx.send(event.unwrap()).await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        events
    });
    app.post(
        "/upload",
        upload.with(middleware::body_limit(50 * 1024 * 1024)),
    );

    println!("⚡ http://localhost:3000");
    app.run(3000)
}
