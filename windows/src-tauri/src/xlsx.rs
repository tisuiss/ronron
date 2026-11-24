//! A small .xlsx writer: one sheet, a bold header row, then rows of text and
//! dates (real Excel dates, so they sort and filter). An .xlsx file is a zip of
//! a few XML parts; they are stored uncompressed, which every spreadsheet
//! reads. No crate needed for a table this size.

/// One cell of a row.
pub enum Cell {
    Text(String),
    /// A calendar day, shown in the reader's own date format.
    Date(chrono::NaiveDate),
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 refuses most control characters.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// "A", "B"… "Z", "AA"…
fn column(mut i: usize) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            return name;
        }
        i = i / 26 - 1;
    }
}

/// Excel's day number: days since 30 December 1899.
fn serial(date: chrono::NaiveDate) -> i64 {
    let epoch = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).expect("a valid date");
    (date - epoch).num_days()
}

fn sheet(header: &[&str], widths: &[f64], rows: &[Vec<Cell>]) -> String {
    let mut x = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><cols>"#);
    for (i, w) in widths.iter().enumerate() {
        x.push_str(&format!(r#"<col min="{n}" max="{n}" width="{w}" customWidth="1"/>"#, n = i + 1));
    }
    x.push_str("</cols><sheetData>");
    x.push_str(r#"<row r="1">"#);
    for (i, h) in header.iter().enumerate() {
        x.push_str(&format!(r#"<c r="{}1" s="2" t="inlineStr"><is><t>{}</t></is></c>"#, column(i), escape(h)));
    }
    x.push_str("</row>");
    for (r, row) in rows.iter().enumerate() {
        let n = r + 2;
        x.push_str(&format!(r#"<row r="{n}">"#));
        for (i, cell) in row.iter().enumerate() {
            let at = format!("{}{n}", column(i));
            match cell {
                Cell::Text(s) => x.push_str(&format!(r#"<c r="{at}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#, escape(s))),
                Cell::Date(d) => x.push_str(&format!(r#"<c r="{at}" s="1"><v>{}</v></c>"#, serial(*d))),
            }
        }
        x.push_str("</row>");
    }
    x.push_str("</sheetData>");
    if !rows.is_empty() {
        x.push_str(&format!(r#"<autoFilter ref="A1:{}{}"/>"#, column(header.len().saturating_sub(1)), rows.len() + 1));
    }
    x.push_str("</worksheet>");
    x
}

/// The workbook's bytes: one sheet named `name`.
pub fn workbook(name: &str, header: &[&str], widths: &[f64], rows: &[Vec<Cell>]) -> Vec<u8> {
    let name: String = escape(name).chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect();
    let parts: Vec<(&str, String)> = vec![
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#.into()),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.into()),
        ("xl/workbook.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{}" sheetId="1" r:id="rId1"/></sheets></workbook>"#, if name.is_empty() { "Sheet1".into() } else { name })),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.into()),
        // Style 1: a date in the reader's short date format (built-in 14). Style 2: bold.
        ("xl/styles.xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="3"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs></styleSheet>"#.into()),
        ("xl/worksheets/sheet1.xml", sheet(header, widths, rows)),
    ];
    zip_stored(&parts.iter().map(|(n, d)| (*n, d.as_bytes())).collect::<Vec<_>>())
}

// ── A zip of stored (uncompressed) entries ────────────────────────────────────

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn zip_stored(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    // 1 January 2026, 00:00 in MS-DOS form: any valid date will do.
    let (time, date) = (0u16, ((2026 - 1980) << 9 | 1 << 5 | 1) as u16);
    for (name, data) in files {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name = name.as_bytes();
        let mut head = Vec::new();
        head.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        head.extend_from_slice(&20u16.to_le_bytes()); // version needed
        head.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        head.extend_from_slice(&0u16.to_le_bytes()); // stored
        head.extend_from_slice(&time.to_le_bytes());
        head.extend_from_slice(&date.to_le_bytes());
        head.extend_from_slice(&crc.to_le_bytes());
        head.extend_from_slice(&size.to_le_bytes());
        head.extend_from_slice(&size.to_le_bytes());
        head.extend_from_slice(&(name.len() as u16).to_le_bytes());
        head.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&head);
        out.extend_from_slice(name);
        out.extend_from_slice(data);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // made by
        central.extend_from_slice(&head[4..30]); // version needed … extra length
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        central.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let start = out.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_is_the_zip_one() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn columns_are_named_as_in_excel() {
        assert_eq!(column(0), "A");
        assert_eq!(column(25), "Z");
        assert_eq!(column(26), "AA");
        assert_eq!(column(27), "AB");
    }

    #[test]
    fn dates_are_excel_day_numbers() {
        assert_eq!(serial(chrono::NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()), 2);
        assert_eq!(serial(chrono::NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()), 46350);
    }

    #[test]
    fn a_workbook_is_a_zip_with_every_part_and_escaped_text() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        let bytes = workbook("Clockify", &["Date", "Task", "Description"], &[12.0, 30.0, 50.0], &[
            vec![Cell::Date(d), Cell::Text("149-0-004".into()), Cell::Text("Mails & <calls>".into())],
        ]);
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let text = String::from_utf8_lossy(&bytes);
        for part in ["[Content_Types].xml", "_rels/.rels", "xl/workbook.xml", "xl/styles.xml", "xl/worksheets/sheet1.xml"] {
            assert!(text.contains(part), "{part}");
        }
        assert!(text.contains("Mails &amp; &lt;calls&gt;"));
        assert!(text.contains(r#"<c r="A2" s="1"><v>46350</v></c>"#));
        assert!(text.contains(r#"<autoFilter ref="A1:C2"/>"#));
        // The end of central directory says six entries.
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(&end[..4], &0x0605_4b50u32.to_le_bytes());
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 6);
    }
}
