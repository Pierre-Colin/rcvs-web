use std::io::BufRead;

use actix_files::NamedFile;
use actix_web::{HttpResponse, Responder};

use super::*;

fn read_page(file_name: &str) -> actix_web::Result<NamedFile> {
    let path: std::path::PathBuf = file_name.parse()?;
    let file = NamedFile::open(path)?
        .set_content_type("text/html; charset=utf-8".parse::<mime::Mime>().unwrap());
    Ok(file)
}

fn preprocess_page(file_name: &str, name: &str) -> actix_web::HttpResponse {
    let file = match std::fs::File::open(file_name) {
        Ok(f) => f,
        Err(what) => {
            return HttpResponse::InternalServerError()
                .body(&format!("Failed to open file: {}", what))
        }
    };
    let lines = std::io::BufReader::new(file).lines();
    let mut processed = String::new();
    for line in lines {
        match line {
            Ok(line) => {
                let s = line.replace("$NAME", name);
                processed.push_str(&s);
            }
            Err(what) => {
                return HttpResponse::InternalServerError()
                    .body(&format!("Failed to preprocess file: {}", what))
            }
        }
    }
    HttpResponse::Ok()
        .set_header(
            actix_web::http::header::CONTENT_TYPE,
            "text/html; charset=utf-8",
        )
        .body(processed)
}

pub async fn vote() -> actix_web::Result<NamedFile> {
    read_page("vote.html")
}

pub async fn result() -> actix_web::Result<NamedFile> {
    read_page("result.html")
}

pub async fn about(state: SharedState) -> impl Responder {
    let state_lock = match state.read() {
        Ok(lock) => lock,
        Err(what) => {
            return HttpResponse::InternalServerError().body(&format!("Mutex poisoned: {}", what))
        }
    };
    preprocess_page("about.html", (*state_lock).get_title())
}
