//! The single profile: what is stored, and what a CV file yields.

use std::io::Write;
use std::path::PathBuf;

use talkcer_core::{
    connect_and_migrate, cv_prompt, parse_cv_fields, read_cv, CareerEntry, CvSource, EducationEntry,
    ProfileService, SaveProfile,
};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

fn db_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

fn profile(name: &str) -> SaveProfile {
    SaveProfile {
        name: name.to_owned(),
        birth_date: "1990-01-01".to_owned(),
        birth_place: "Bandung".to_owned(),
        phone: "0812".to_owned(),
        email: "ada@example.com".to_owned(),
        github: "https://github.com/ada".to_owned(),
        linkedin: "https://linkedin.com/in/ada".to_owned(),
        career: vec![CareerEntry {
            company: "Acme".to_owned(),
            role: "Engineer".to_owned(),
            period: "2020-2024".to_owned(),
            summary: "Built the API.".to_owned(),
        }],
        education: vec![EducationEntry {
            institution: "ITB".to_owned(),
            credential: "Informatika".to_owned(),
            period: "2008-2012".to_owned(),
        }],
        skills: vec!["Rust".to_owned(), " ".to_owned()],
    }
}

#[tokio::test]
async fn profile_round_trips_and_drops_empty_entries() {
    let file = db_path("profile");
    let pool = connect_and_migrate(&file).await.unwrap();
    let service = ProfileService::new(pool);

    let blank = service.get().await.unwrap();
    assert!(blank.name.is_empty());

    let saved = service.save(profile("  Ada   ")).await.unwrap();
    assert_eq!(saved.name, "Ada");
    assert_eq!(saved.career.len(), 1);
    assert_eq!(saved.skills, vec!["Rust".to_owned()]);

    let again = service.get().await.unwrap();
    assert_eq!(again.email, "ada@example.com");
    let context = again.context_text();
    assert!(context.contains("Name: Ada"));
    assert!(context.contains("Work: Engineer, Acme, 2020-2024"));
    assert!(!context.contains("Bandung"), "birth place stays out of the model context");

    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn oversized_field_is_rejected() {
    let file = db_path("profile-long");
    let pool = connect_and_migrate(&file).await.unwrap();
    let service = ProfileService::new(pool);
    let mut input = profile("Ada");
    input.name = "a".repeat(300);
    assert!(service.save(input).await.is_err());
    let _ = std::fs::remove_file(file);
}

#[test]
fn docx_text_is_read_and_the_file_is_not_stored() {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        zip.start_file("word/document.xml", SimpleFileOptions::default()).unwrap();
        write!(
            zip,
            r#"<w:document><w:p><w:r><w:t>Ada Lovelace</w:t></w:r></w:p><w:p><w:r><w:t>Built engines.</w:t></w:r></w:p></w:document>"#
        )
        .unwrap();
        zip.finish().unwrap();
    }
    let draft = read_cv("cv.docx", &buffer.into_inner()).unwrap();
    assert_eq!(draft.source, CvSource::FileText);
    assert!(draft.text.contains("Ada Lovelace"));
    assert!(draft.text.contains("Built engines."));
    assert!(draft.warning.is_none());
}

#[test]
fn scanned_pdf_reports_no_text_instead_of_guessing() {
    let draft = read_cv("scan.pdf", b"%PDF-1.4 image only").unwrap();
    assert!(draft.text.is_empty());
    assert!(draft.warning.unwrap().contains("scanned"));
}

#[test]
fn model_reply_fills_fields_and_never_guesses_birth() {
    let raw = r#"```json
    {"name":"Ada","email":"ada@example.com","birth_date":"1990-01-01",
     "career":[{"company":"Acme","role":"Engineer","period":"2020","summary":""}],
     "skills":["Rust"]}
    ```"#;
    let fields = parse_cv_fields(raw);
    assert_eq!(fields.name, "Ada");
    assert!(fields.birth_date.is_empty(), "birth date is dropped even if the model invented one");
    assert_eq!(fields.career[0].company, "Acme");
    assert_eq!(fields.skills, vec!["Rust".to_owned()]);
    assert!(cv_prompt("text").contains("Never guess a birth date"));
}

#[test]
fn unknown_file_type_is_rejected() {
    assert!(read_cv("cv.exe", b"MZ").is_err());
    assert!(read_cv("cv.txt", b"").is_err());
}
