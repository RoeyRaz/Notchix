//! Bounded local CSV/TSV quick profiling. No data leaves this process.

use std::{io::Read, path::Path};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ROWS: usize = 250_000;
const MAX_COLUMNS: usize = 128;

#[derive(Clone, Debug)]
pub struct NumericSummary {
    pub name: String,
    pub count: usize,
    pub missing: usize,
    pub min: f64,
    pub mean: f64,
    pub max: f64,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub file_name: String,
    pub rows: usize,
    pub columns: usize,
    pub missing_cells: usize,
    pub numeric: Vec<NumericSummary>,
}

pub fn profile(path: &Path) -> Result<Profile, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("Cannot read file: {e}"))?;
    if !metadata.is_file() {
        return Err("Choose a CSV or TSV file.".into());
    }
    if metadata.len() > MAX_BYTES {
        return Err("This quick profile is limited to files up to 32 MB.".into());
    }
    let delimiter = match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" => b',',
        "tsv" | "tab" => b'\t',
        _ => return Err("Choose a .csv or .tsv file.".into()),
    };
    let file = std::fs::File::open(path).map_err(|e| format!("Could not read file: {e}"))?;
    let mut bytes = Vec::with_capacity(metadata.len().min(MAX_BYTES) as usize);
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Could not read file: {e}"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("This quick profile is limited to files up to 32 MB.".into());
    }
    let content =
        std::str::from_utf8(&bytes).map_err(|e| format!("Could not read UTF-8 CSV data: {e}"))?;

    let mut headers: Option<Vec<String>> = None;
    let mut numeric: Vec<Option<(usize, f64, f64, f64)>> = Vec::new();
    let mut valid_numeric = Vec::new();
    let mut missing_by_col = Vec::new();
    let mut rows = 0usize;
    let mut missing_cells = 0usize;
    parse_delimited(content, delimiter, |row| {
        if headers.is_none() {
            if row.is_empty() || row.len() > MAX_COLUMNS {
                return Err("The file must have between 1 and 128 columns.".into());
            }
            let width = row.len();
            headers = Some(row);
            numeric.resize(width, None);
            valid_numeric.resize(width, true);
            missing_by_col.resize(width, 0usize);
            return Ok(());
        }
        rows += 1;
        if rows > MAX_ROWS {
            return Err("This quick profile is limited to 250,000 data rows.".into());
        }
        for col in 0..headers.as_ref().unwrap().len() {
            let value = row.get(col).map_or("", String::as_str).trim();
            if value.is_empty() {
                missing_cells += 1;
                missing_by_col[col] += 1;
            } else if valid_numeric[col] {
                match value.parse::<f64>() {
                    Ok(value) if value.is_finite() => {
                        numeric[col] = Some(match numeric[col] {
                            Some((count, sum, min, max)) => {
                                (count + 1, sum + value, min.min(value), max.max(value))
                            }
                            None => (1, value, value, value),
                        });
                    }
                    _ => {
                        valid_numeric[col] = false;
                        numeric[col] = None;
                    }
                }
            }
        }
        Ok(())
    })?;
    let headers = headers.ok_or_else(|| "The file is empty.".to_string())?;
    let numeric = numeric
        .into_iter()
        .enumerate()
        .filter_map(|(col, item)| {
            if !valid_numeric[col] {
                return None;
            }
            let (count, sum, min, max) = item?;
            Some(NumericSummary {
                name: headers[col].clone(),
                count,
                missing: missing_by_col[col],
                min,
                mean: sum / count as f64,
                max,
            })
        })
        .collect();
    Ok(Profile {
        file_name: path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        rows,
        columns: headers.len(),
        missing_cells,
        numeric,
    })
}

fn parse_delimited(
    input: &str,
    delimiter: u8,
    mut record: impl FnMut(Vec<String>) -> Result<(), String>,
) -> Result<(), String> {
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' && field.is_empty() {
            quoted = true;
        } else if ch == delimiter as char {
            if row.len() >= MAX_COLUMNS {
                return Err("The file must have no more than 128 columns.".into());
            }
            row.push(std::mem::take(&mut field));
        } else if ch == '\n' || ch == '\r' {
            if ch == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if row.len() >= MAX_COLUMNS {
                return Err("The file must have no more than 128 columns.".into());
            }
            row.push(std::mem::take(&mut field));
            record(std::mem::take(&mut row))?;
        } else {
            field.push(ch);
        }
    }
    if quoted {
        return Err("A quoted field is not closed.".into());
    }
    if !field.is_empty() || !row.is_empty() {
        if row.len() >= MAX_COLUMNS {
            return Err("The file must have no more than 128 columns.".into());
        }
        row.push(field);
        record(row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_numeric_and_missing_summaries() {
        let path = std::env::temp_dir().join(format!("notchix-profile-{}.csv", std::process::id()));
        std::fs::write(&path, "name,value\na,1\nb,3\nc,\n").unwrap();
        let got = profile(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(got.rows, 3);
        assert_eq!(got.columns, 2);
        assert_eq!(got.missing_cells, 1);
        assert_eq!(got.numeric.len(), 1);
        assert_eq!(got.numeric[0].mean, 2.0);
        assert_eq!(got.numeric[0].missing, 1);
    }

    #[test]
    fn rejects_non_tabular_extensions() {
        let path = std::env::temp_dir().join("notchix-profile.txt");
        std::fs::write(&path, "a,b\n1,2").unwrap();
        assert!(profile(&path).unwrap_err().contains(".csv or .tsv"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_quoted_commas_and_newlines() {
        let mut rows = Vec::new();
        parse_delimited("name,value\n\"A, B\",2\n\"multi\nline\",4\n", b',', |r| {
            rows.push(r);
            Ok(())
        })
        .unwrap();
        assert_eq!(rows[1][0], "A, B");
        assert_eq!(rows[2][0], "multi\nline");
    }

    #[test]
    fn mixed_text_columns_are_not_reported_as_numeric() {
        let path = std::env::temp_dir().join(format!("notchix-mixed-{}.csv", std::process::id()));
        std::fs::write(&path, "value\n1\nword\n3\n").unwrap();
        let got = profile(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(got.numeric.is_empty());
    }
}
