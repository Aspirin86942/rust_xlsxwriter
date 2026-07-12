// Workbook unit tests.
//
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright 2022-2026, John McNamara, jmcnamara@cpan.org

#[cfg(test)]
mod workbook_tests {

    use crate::{test_functions::xml_to_vec, XlsxError};
    use crate::{xmlwriter, Table, Workbook};
    use pretty_assertions::assert_eq;

    #[cfg(feature = "constant_memory")]
    use crate::worksheet::{TempIoFailurePoint, Worksheet};

    #[test]
    fn test_assemble() {
        let mut workbook = Workbook::default();
        workbook.add_worksheet();

        workbook.assemble_xml_file();

        let got = xmlwriter::cursor_to_str(&workbook.writer);
        let got = xml_to_vec(got);

        let expected = xml_to_vec(
            r#"
            <?xml version="1.0" encoding="UTF-8" standalone="yes"?>
            <workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
              <fileVersion appName="xl" lastEdited="4" lowestEdited="4" rupBuild="4505"/>
              <workbookPr defaultThemeVersion="124226"/>
              <bookViews>
                <workbookView xWindow="240" yWindow="15" windowWidth="16095" windowHeight="9660"/>
              </bookViews>
              <sheets>
                <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
              </sheets>
              <calcPr calcId="124519" fullCalcOnLoad="1"/>
            </workbook>
            "#,
        );

        assert_eq!(expected, got);
    }

    #[test]
    fn define_name() {
        let mut workbook = Workbook::default();

        // Test invalid defined names.
        let names = vec![
            ".foo",    // Invalid start character.
            "foo bar", // Space in name
            "Foo,",    // Other invalid characters.
            "Foo/", "Foo[", "Foo]", "Foo'", "Foo\"bar", "Foo:", "Foo*",
        ];

        for name in names {
            let result = workbook.define_name(name, "");
            assert!(matches!(result, Err(XlsxError::ParameterError(_))));
        }
    }

    #[test]
    fn duplicate_worksheets() {
        let mut workbook = Workbook::default();

        let _ = workbook.add_worksheet().set_name("Foo").unwrap();
        let _ = workbook.add_worksheet().set_name("Foo").unwrap();

        let result = workbook.save_to_buffer();
        assert!(matches!(result, Err(XlsxError::SheetnameReused(_))));
    }

    #[test]
    fn duplicate_worksheets_case_insensitive() {
        let mut workbook = Workbook::default();

        let _ = workbook.add_worksheet().set_name("Foo").unwrap();
        let _ = workbook.add_worksheet().set_name("foo").unwrap();

        let result = workbook.save_to_buffer();
        assert!(matches!(result, Err(XlsxError::SheetnameReused(_))));
    }

    #[test]
    fn compression_level_accepts_only_zip_backend_range() {
        let mut workbook = Workbook::new();

        assert_eq!(workbook.compression_level, None);
        assert!(workbook.set_compression_level(1).is_ok());
        assert_eq!(workbook.compression_level, Some(1));
        assert!(workbook.set_compression_level(9).is_ok());
        assert_eq!(workbook.compression_level, Some(9));
        assert!(matches!(
            workbook.set_compression_level(0),
            Err(XlsxError::ParameterError(_))
        ));
        assert!(matches!(
            workbook.set_compression_level(10),
            Err(XlsxError::ParameterError(_))
        ));
    }

    #[test]
    fn duplicate_tables() {
        let mut workbook = Workbook::default();
        let worksheet = workbook.add_worksheet();

        let mut table = Table::new().set_name("Foo");

        worksheet.add_table(0, 0, 9, 9, &table).unwrap();

        table = table.set_name("foo");
        worksheet.add_table(10, 10, 19, 19, &table).unwrap();

        let result = workbook.prepare_tables();

        assert!(matches!(result, Err(XlsxError::TableNameReused(_))));
    }

    #[test]
    fn non_xml_theme() {
        let mut workbook = Workbook::default();
        let theme_file = "tests/input/themes/empty.xml";

        let result = workbook.use_custom_theme(theme_file);

        assert!(matches!(result, Err(XlsxError::ThemeError(_))));
    }

    #[test]
    fn image_gradient_fills_in_theme() {
        let mut workbook = Workbook::default();
        let theme_file = "tests/input/themes/civic.xml";

        let result = workbook.use_custom_theme(theme_file);

        assert!(matches!(result, Err(XlsxError::ThemeError(_))));
    }

    #[test]
    fn no_theme_file_in_zip() {
        let mut workbook = Workbook::default();
        let theme_file = "tests/input/themes/no_theme.zip";

        let result = workbook.use_custom_theme(theme_file);

        assert!(matches!(result, Err(XlsxError::ThemeError(_))));
    }

    #[cfg(feature = "constant_memory")]
    fn assert_storage_full(error: XlsxError) {
        let XlsxError::IoError(source) = error else {
            panic!("expected IoError");
        };
        assert_eq!(source.kind(), std::io::ErrorKind::StorageFull);
        assert_eq!(source.raw_os_error(), Some(112));
    }

    #[cfg(feature = "constant_memory")]
    fn assert_create_failure_is_recoverable(worksheet: &mut Worksheet) {
        worksheet.file_writer.failure_point = Some(TempIoFailurePoint::Create);
        worksheet.write_number(0, 0, 1).unwrap();

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            worksheet.write_number(1, 0, 2).map(|_| ())
        }));
        assert!(outcome.is_ok(), "temporary file creation must not panic");
        let error = match outcome.unwrap() {
            Ok(_) => panic!("expected temporary file creation failure"),
            Err(error) => error,
        };
        assert_storage_full(error);
    }

    #[cfg(feature = "constant_memory")]
    fn assert_save_failure_is_recoverable(failure_point: TempIoFailurePoint) {
        let mut workbook = Workbook::new();
        let worksheet = workbook.add_worksheet_with_low_memory();
        worksheet.write_number(0, 0, 1).unwrap();
        worksheet.file_writer.failure_point = Some(failure_point);

        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| workbook.save_to_buffer()));
        assert!(outcome.is_ok(), "temporary file I/O must not panic");
        let error = outcome
            .unwrap()
            .expect_err("expected temporary file I/O failure");
        assert_storage_full(error);
    }

    #[cfg(feature = "constant_memory")]
    fn read_sheet_xml(buffer: Vec<u8>) -> String {
        use std::io::Read;

        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(buffer)).unwrap();
        let mut xml = String::new();
        archive
            .by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut xml)
            .unwrap();
        xml
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn standard_worksheet_never_initializes_temp_writer() {
        let mut worksheet = Worksheet::new();
        assert!(!worksheet.file_writer.is_initialized());

        worksheet.write_number(0, 0, 1).unwrap();
        worksheet.write_number(1, 0, 2).unwrap();

        assert!(!worksheet.file_writer.is_initialized());
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn all_memory_factories_defer_tempfile_creation() {
        let mut workbook = Workbook::new();
        assert!(!workbook
            .add_worksheet_with_constant_memory()
            .file_writer
            .is_initialized());

        let mut workbook = Workbook::new();
        assert!(!workbook
            .add_worksheet_with_low_memory()
            .file_writer
            .is_initialized());

        let mut workbook = Workbook::new();
        assert!(!workbook
            .new_worksheet_with_constant_memory()
            .file_writer
            .is_initialized());

        let mut workbook = Workbook::new();
        assert!(!workbook
            .new_worksheet_with_low_memory()
            .file_writer
            .is_initialized());
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn add_constant_memory_create_failure_is_recoverable() {
        let mut workbook = Workbook::new();
        assert_create_failure_is_recoverable(workbook.add_worksheet_with_constant_memory());
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn add_low_memory_create_failure_is_recoverable() {
        let mut workbook = Workbook::new();
        assert_create_failure_is_recoverable(workbook.add_worksheet_with_low_memory());
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn new_constant_memory_create_failure_is_recoverable() {
        let mut workbook = Workbook::new();
        let mut worksheet = workbook.new_worksheet_with_constant_memory();
        assert_create_failure_is_recoverable(&mut worksheet);
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn new_low_memory_create_failure_is_recoverable() {
        let mut workbook = Workbook::new();
        let mut worksheet = workbook.new_worksheet_with_low_memory();
        assert_create_failure_is_recoverable(&mut worksheet);
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn flush_failure_returns_original_io_error() {
        assert_save_failure_is_recoverable(TempIoFailurePoint::Flush);
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn rewind_failure_returns_original_io_error() {
        assert_save_failure_is_recoverable(TempIoFailurePoint::Rewind);
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn packager_copy_failure_returns_original_io_error() {
        assert_save_failure_is_recoverable(TempIoFailurePoint::ReadDuringCopy);
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn removed_tempdir_after_set_tempdir_returns_io_error() {
        let root = tempfile::tempdir().unwrap();
        let controlled_tempdir = root.path().join("controlled");
        std::fs::create_dir(&controlled_tempdir).unwrap();

        let mut workbook = Workbook::new();
        workbook.set_tempdir(&controlled_tempdir).unwrap();
        std::fs::remove_dir(&controlled_tempdir).unwrap();
        let expected = tempfile::tempfile_in(&controlled_tempdir).unwrap_err();

        let worksheet = workbook.add_worksheet_with_low_memory();
        worksheet.write_number(0, 0, 1).unwrap();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            worksheet.write_number(1, 0, 2).map(|_| ())
        }));
        assert!(outcome.is_ok(), "removed tempdir must not panic");
        let error = match outcome.unwrap() {
            Ok(_) => panic!("expected removed tempdir I/O failure"),
            Err(error) => error,
        };
        let XlsxError::IoError(source) = error else {
            panic!("expected IoError");
        };
        assert_eq!(source.kind(), expected.kind());
        assert_eq!(source.raw_os_error(), expected.raw_os_error());
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn empty_single_and_multi_row_low_memory_sheets_round_trip() {
        let mut empty = Workbook::new();
        empty.add_worksheet_with_low_memory();
        let empty_xml = read_sheet_xml(empty.save_to_buffer().unwrap());
        assert!(empty_xml.contains("<sheetData"));
        assert!(!empty_xml.contains("<row"));

        let mut single = Workbook::new();
        single
            .add_worksheet_with_low_memory()
            .write_number(0, 0, 1)
            .unwrap();
        let single_xml = read_sheet_xml(single.save_to_buffer().unwrap());
        assert!(single_xml.contains("<v>1</v>"));
        assert_eq!(single_xml.matches("<row").count(), 1);

        let mut multi = Workbook::new();
        let worksheet = multi.add_worksheet_with_low_memory();
        worksheet.write_number(0, 0, 1).unwrap();
        worksheet.write_number(1, 0, 2).unwrap();
        worksheet.write_number(2, 0, 3).unwrap();
        let multi_xml = read_sheet_xml(multi.save_to_buffer().unwrap());
        assert!(multi_xml.contains("<v>1</v>"));
        assert!(multi_xml.contains("<v>2</v>"));
        assert!(multi_xml.contains("<v>3</v>"));
        assert_eq!(multi_xml.matches("<row").count(), 3);
    }

    #[test]
    #[cfg(feature = "constant_memory")]
    fn controlled_tempdir_does_not_touch_process_temp_canary() {
        const CHILD_MARKER: &str = "RUST_XLSXWRITER_TEMP_CANARY_CHILD";
        const CONTROLLED_DIR: &str = "RUST_XLSXWRITER_CONTROLLED_TEMPDIR";

        if std::env::var_os(CHILD_MARKER).is_some() {
            let controlled_tempdir = std::path::PathBuf::from(
                std::env::var_os(CONTROLLED_DIR).expect("controlled tempdir is set"),
            );

            let mut workbook = Workbook::new();
            workbook
                .add_worksheet()
                .write_string(0, 0, "standard")
                .unwrap();
            workbook.set_tempdir(&controlled_tempdir).unwrap();
            let worksheet = workbook.add_worksheet_with_low_memory();
            worksheet.write_string(0, 0, "low memory").unwrap();
            workbook.save_to_buffer().unwrap();
            return;
        }

        let root = tempfile::tempdir().unwrap();
        let controlled_tempdir = root.path().join("controlled");
        std::fs::create_dir(&controlled_tempdir).unwrap();
        let canary = root.path().join("missing-process-temp");
        let test_name = "workbook::tests::workbook_tests::controlled_tempdir_does_not_touch_process_temp_canary";

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(test_name)
            .arg("--nocapture")
            .env(CHILD_MARKER, "1")
            .env(CONTROLLED_DIR, &controlled_tempdir)
            .env("TEMP", &canary)
            .env("TMP", &canary)
            .env("TMPDIR", &canary)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "child process failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!canary.exists());
    }
}
