//! The one personal profile the assistant can draw on.
//!
//! The CV file used for autofill is read and thrown away. Only the fields the
//! user keeps are stored.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::error::CoreError;

const ROW_ID: &str = "default";
const MAX_CV_BYTES: usize = 5 * 1024 * 1024;

/// One job in the career history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CareerEntry {
    /// Employer.
    pub company: String,
    /// Job title.
    pub role: String,
    /// When the job ran, as written.
    pub period: String,
    /// What the work covered.
    pub summary: String,
}

/// One school or course.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EducationEntry {
    /// School or institution.
    pub institution: String,
    /// Field or credential.
    pub credential: String,
    /// When it ran, as written.
    pub period: String,
}

/// The stored profile. Empty strings mean the user has not filled that field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Full name.
    pub name: String,
    /// Birth date, kept as written.
    pub birth_date: String,
    /// Birth place.
    pub birth_place: String,
    /// Phone number.
    pub phone: String,
    /// Email address.
    pub email: String,
    /// GitHub profile URL.
    pub github: String,
    /// LinkedIn profile URL.
    pub linkedin: String,
    /// Jobs, newest first as the user arranged them.
    pub career: Vec<CareerEntry>,
    /// Schools and courses.
    pub education: Vec<EducationEntry>,
    /// Skills, one per entry.
    pub skills: Vec<String>,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

impl Profile {
    /// A profile with nothing filled in.
    #[must_use]
    pub fn blank(updated_at: DateTime<Utc>) -> Self {
        Self {
            name: String::new(),
            birth_date: String::new(),
            birth_place: String::new(),
            phone: String::new(),
            email: String::new(),
            github: String::new(),
            linkedin: String::new(),
            career: Vec::new(),
            education: Vec::new(),
            skills: Vec::new(),
            updated_at,
        }
    }

    /// Text a session can hand to the model when the user asks for profile context.
    ///
    /// Birth date and birth place are left out on purpose: they are personal and
    /// rarely useful to an answer.
    #[must_use]
    pub fn context_text(&self) -> String {
        let mut lines = Vec::new();
        push(&mut lines, "Name", &self.name);
        push(&mut lines, "Phone", &self.phone);
        push(&mut lines, "Email", &self.email);
        push(&mut lines, "GitHub", &self.github);
        push(&mut lines, "LinkedIn", &self.linkedin);
        for job in &self.career {
            let header = join_present([&job.role, &job.company, &job.period]);
            if !header.is_empty() || !job.summary.trim().is_empty() {
                lines.push(format!("Work: {header}"));
                if !job.summary.trim().is_empty() {
                    lines.push(format!("  {}", job.summary.trim()));
                }
            }
        }
        for school in &self.education {
            let header = join_present([&school.credential, &school.institution, &school.period]);
            if !header.is_empty() {
                lines.push(format!("Education: {header}"));
            }
        }
        let skills: Vec<&str> = self.skills.iter().map(|skill| skill.trim()).filter(|skill| !skill.is_empty()).collect();
        if !skills.is_empty() {
            lines.push(format!("Skills: {}", skills.join(", ")));
        }
        lines.join("\n")
    }
}

fn push(lines: &mut Vec<String>, label: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        lines.push(format!("{label}: {value}"));
    }
}

fn join_present<'a>(parts: impl IntoIterator<Item = &'a String>) -> String {
    parts
        .into_iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Loads and stores the single profile row.
#[derive(Clone)]
pub struct ProfileService {
    pool: SqlitePool,
}

impl ProfileService {
    /// Create a service over the shared pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Return the saved profile, creating the empty row when needed.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn get(&self) -> Result<Profile, CoreError> {
        self.ensure().await?;
        self.load().await
    }

    /// Replace the profile with what the form sent.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidProfile`] when a value is absurdly long.
    pub async fn save(&self, input: SaveProfile) -> Result<Profile, CoreError> {
        self.ensure().await?;
        let profile = input.validated()?;
        sqlx::query(
            "UPDATE profile SET name = ?2, birth_date = ?3, birth_place = ?4, phone = ?5,
                    email = ?6, github = ?7, linkedin = ?8, career = ?9, education = ?10,
                    skills = ?11, updated_at = ?12
             WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(&profile.name)
        .bind(&profile.birth_date)
        .bind(&profile.birth_place)
        .bind(&profile.phone)
        .bind(&profile.email)
        .bind(&profile.github)
        .bind(&profile.linkedin)
        .bind(serde_json::to_string(&profile.career).unwrap_or_else(|_| "[]".to_owned()))
        .bind(serde_json::to_string(&profile.education).unwrap_or_else(|_| "[]".to_owned()))
        .bind(serde_json::to_string(&profile.skills).unwrap_or_else(|_| "[]".to_owned()))
        .bind(profile.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    async fn ensure(&self) -> Result<(), CoreError> {
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM profile WHERE id = ?1")
            .bind(ROW_ID)
            .fetch_optional(&self.pool)
            .await?;
        if exists.is_some() {
            return Ok(());
        }
        sqlx::query("INSERT INTO profile (id, updated_at) VALUES (?1, ?2)")
            .bind(ROW_ID)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn load(&self) -> Result<Profile, CoreError> {
        let row = sqlx::query(
            "SELECT name, birth_date, birth_place, phone, email, github, linkedin,
                    career, education, skills, updated_at
             FROM profile WHERE id = ?1",
        )
        .bind(ROW_ID)
        .fetch_one(&self.pool)
        .await?;
        let updated: String = row.try_get("updated_at")?;
        Ok(Profile {
            name: row.try_get("name")?,
            birth_date: row.try_get("birth_date")?,
            birth_place: row.try_get("birth_place")?,
            phone: row.try_get("phone")?,
            email: row.try_get("email")?,
            github: row.try_get("github")?,
            linkedin: row.try_get("linkedin")?,
            career: parse_json(row.try_get("career")?),
            education: parse_json(row.try_get("education")?),
            skills: parse_json(row.try_get("skills")?),
            updated_at: DateTime::parse_from_rfc3339(&updated)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))?,
        })
    }
}

/// The form payload. Values are trimmed and length-checked before saving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveProfile {
    /// Full name.
    #[serde(default)]
    pub name: String,
    /// Birth date.
    #[serde(default)]
    pub birth_date: String,
    /// Birth place.
    #[serde(default)]
    pub birth_place: String,
    /// Phone number.
    #[serde(default)]
    pub phone: String,
    /// Email address.
    #[serde(default)]
    pub email: String,
    /// GitHub profile URL.
    #[serde(default)]
    pub github: String,
    /// LinkedIn profile URL.
    #[serde(default)]
    pub linkedin: String,
    /// Jobs.
    #[serde(default)]
    pub career: Vec<CareerEntry>,
    /// Schools and courses.
    #[serde(default)]
    pub education: Vec<EducationEntry>,
    /// Skills.
    #[serde(default)]
    pub skills: Vec<String>,
}

impl SaveProfile {
    fn validated(self) -> Result<Profile, CoreError> {
        let mut profile = Profile::blank(Utc::now());
        profile.name = bounded("name", self.name, 200)?;
        profile.birth_date = bounded("birth date", self.birth_date, 40)?;
        profile.birth_place = bounded("birth place", self.birth_place, 200)?;
        profile.phone = bounded("phone", self.phone, 40)?;
        profile.email = bounded("email", self.email, 200)?;
        profile.github = bounded("github", self.github, 300)?;
        profile.linkedin = bounded("linkedin", self.linkedin, 300)?;
        profile.career = self
            .career
            .into_iter()
            .filter_map(|entry| clean_career(entry).transpose())
            .collect::<Result<_, _>>()?;
        profile.education = self
            .education
            .into_iter()
            .filter_map(|entry| clean_education(entry).transpose())
            .collect::<Result<_, _>>()?;
        profile.skills = self
            .skills
            .into_iter()
            .map(|skill| bounded("skill", skill, 80))
            .filter(|skill| skill.as_ref().map_or(true, |value| !value.is_empty()))
            .collect::<Result<_, _>>()?;
        if profile.career.len() > 50 || profile.education.len() > 30 || profile.skills.len() > 200 {
            return Err(CoreError::InvalidProfile("too many entries".to_owned()));
        }
        Ok(profile)
    }
}

fn clean_career(entry: CareerEntry) -> Result<Option<CareerEntry>, CoreError> {
    let entry = CareerEntry {
        company: bounded("company", entry.company, 200)?,
        role: bounded("role", entry.role, 200)?,
        period: bounded("period", entry.period, 80)?,
        summary: bounded("summary", entry.summary, 2_000)?,
    };
    let empty = [&entry.company, &entry.role, &entry.period, &entry.summary]
        .iter()
        .all(|value| value.is_empty());
    Ok((!empty).then_some(entry))
}

fn clean_education(entry: EducationEntry) -> Result<Option<EducationEntry>, CoreError> {
    let entry = EducationEntry {
        institution: bounded("institution", entry.institution, 200)?,
        credential: bounded("credential", entry.credential, 200)?,
        period: bounded("period", entry.period, 80)?,
    };
    let empty = [&entry.institution, &entry.credential, &entry.period]
        .iter()
        .all(|value| value.is_empty());
    Ok((!empty).then_some(entry))
}

fn bounded(field: &str, value: String, max: usize) -> Result<String, CoreError> {
    let value = value.trim().to_owned();
    if value.chars().count() > max {
        return Err(CoreError::InvalidProfile(format!("{field} is too long")));
    }
    Ok(value)
}

fn parse_json<T: Default + for<'de> Deserialize<'de>>(raw: String) -> T {
    serde_json::from_str(&raw).unwrap_or_default()
}

/// A CV reading ready to be shown for confirmation. Nothing is saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CvDraft {
    /// The readable text pulled out of the file.
    pub text: String,
    /// How the text was obtained.
    pub source: CvSource,
    /// Why nothing could be read, when that happened.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Where the CV text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CvSource {
    /// Characters copied straight out of the file.
    FileText,
    /// The model rewrote the text into profile fields.
    LanguageModel,
}

/// Read a CV without keeping the file.
///
/// PDF and DOCX yield their embedded text. A scan, which has no embedded text,
/// comes back with a warning so the user can type instead.
///
/// # Errors
///
/// Returns [`CoreError::InvalidProfile`] for an empty file, an oversized file,
/// or a type that is not PDF, DOCX, or plain text.
pub fn read_cv(filename: &str, bytes: &[u8]) -> Result<CvDraft, CoreError> {
    if bytes.is_empty() {
        return Err(CoreError::InvalidProfile("the file is empty".to_owned()));
    }
    if bytes.len() > MAX_CV_BYTES {
        return Err(CoreError::InvalidProfile("the file is larger than 5 MB".to_owned()));
    }
    let name = filename.to_ascii_lowercase();
    let text = if name.ends_with(".pdf") {
        pdf_text(bytes)
    } else if name.ends_with(".docx") {
        docx_text(bytes)
    } else if name.ends_with(".txt") {
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        return Err(CoreError::InvalidProfile(
            "upload a PDF, DOCX, or text file".to_owned(),
        ));
    };
    match text.map(|value| value.trim().to_owned()).filter(|value| !value.is_empty()) {
        Some(text) => Ok(CvDraft { text, source: CvSource::FileText, warning: None }),
        None => Ok(CvDraft {
            text: String::new(),
            source: CvSource::FileText,
            warning: Some("no text found in the file, so it was probably scanned".to_owned()),
        }),
    }
}

/// The model reply the caller parses into fields.
#[must_use]
pub fn cv_prompt(text: &str) -> String {
    format!(
        "Turn this CV into JSON with exactly these keys: \
         name, phone, email, github, linkedin, \
         career (array of {{company, role, period, summary}}), \
         education (array of {{institution, credential, period}}), \
         skills (array of strings). Use an empty string or empty array when a value \
         is not present. Never guess a birth date or birth place; leave them out. \
         Keep the original wording.\n\n{text}"
    )
}

/// Parse the model's JSON into profile fields. Unknown keys are ignored.
#[must_use]
pub fn parse_cv_fields(raw: &str) -> SaveProfile {
    let json = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let value: Value = serde_json::from_str(json).unwrap_or(Value::Null);
    let object = value.as_object();
    let text = |key: &str| {
        object.and_then(|map| map.get(key)).and_then(Value::as_str).unwrap_or("").to_owned()
    };
    SaveProfile {
        name: text("name"),
        birth_date: String::new(),
        birth_place: String::new(),
        phone: text("phone"),
        email: text("email"),
        github: text("github"),
        linkedin: text("linkedin"),
        career: object
            .and_then(|map| map.get("career"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        education: object
            .and_then(|map| map.get("education"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        skills: object
            .and_then(|map| map.get("skills"))
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
    }
}

/// Pull the literal strings out of a PDF content stream.
///
/// A literal looks like `(text) Tj`. This reads the bytes as text, so a PDF
/// whose text is compressed, or which is a scan, returns nothing and the
/// caller reports that instead of guessing.
fn pdf_text(bytes: &[u8]) -> Option<String> {
    let raw = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut rest = &raw[..];
    while let Some(start) = rest.find('(') {
        rest = &rest[start + 1..];
        let mut end = 0;
        let mut escaped = false;
        let chars: Vec<char> = rest.chars().collect();
        for (index, ch) in chars.iter().enumerate() {
            if escaped {
                escaped = false;
                continue;
            }
            if *ch == '\\' {
                escaped = true;
                continue;
            }
            if *ch == ')' {
                end = index;
                break;
            }
        }
        if end == 0 {
            continue;
        }
        let literal: String = chars[..end].iter().collect();
        let literal = literal.replace("\\n", "\n").replace("\\(", "(").replace("\\)", ")");
        if literal.chars().any(char::is_alphanumeric) {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(literal.trim());
        }
        rest = &rest[end + 1..];
        if out.len() > 100_000 {
            break;
        }
    }
    Some(out).filter(|value| !value.trim().is_empty())
}

/// Pull the text out of a DOCX, which is a zip holding `word/document.xml`.
fn docx_text(bytes: &[u8]) -> Option<String> {
    let file = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut document = archive.by_name("word/document.xml").ok()?;
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut document, &mut xml).ok()?;
    let mut out = String::new();
    for part in xml.split("</w:p>") {
        let mut line = String::new();
        for node in part.split("<w:t") {
            let Some(start) = node.find('>') else { continue };
            let Some(end) = node[start + 1..].find("</w:t>") else { continue };
            line.push_str(&decode_xml(&node[start + 1..start + 1 + end]));
        }
        let line = line.trim().to_owned();
        if !line.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&line);
        }
    }
    Some(out).filter(|value| !value.trim().is_empty())
}

fn decode_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
