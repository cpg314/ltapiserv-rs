static DATA: include_dir::Dir<'_> = include_dir::include_dir!("tests/data");

#[test]
fn main() -> anyhow::Result<()> {
    let checkers =
        ltapiserv_rs::checkers::Checkers::from_archive_bytes(include_bytes!("../en_US.tar.gz"))?;

    for data in DATA.find("*.txt")?.filter_map(|d| d.as_file()) {
        let request =
            ltapiserv_rs::api::Request::new(data.contents_utf8().unwrap().into(), "en-US");
        let suggestions = checkers.suggest(&request.annotations().unwrap());
        let suggestions_expected: Vec<ltapiserv_rs::api::Match> = serde_json::from_slice(
            DATA.get_file(data.path().with_extension("json"))
                .unwrap()
                .contents(),
        )?;
        // std::fs::write(
        //     Path::new("tests/data")
        //         .join(data.path())
        //         .with_extension("json"),
        //     serde_json::to_string_pretty(&suggestions)?,
        // )?;
        pretty_assertions::assert_eq!(suggestions, suggestions_expected);
        println!("{:#?}", suggestions);
    }

    Ok(())
}
