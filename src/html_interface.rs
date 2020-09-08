use actix_files::NamedFile;

fn read_page(file_name: &str) -> actix_web::Result<NamedFile> {
    let path: std::path::PathBuf = file_name.parse()?;
    let file = NamedFile::open(path)?
        .set_content_type("text/html; charset=utf-8".parse::<mime::Mime>().unwrap());
    Ok(file)
}

pub async fn vote() -> actix_web::Result<NamedFile> {
    read_page("vote.html")
}

pub async fn result() -> actix_web::Result<NamedFile> {
    read_page("result.html")
}