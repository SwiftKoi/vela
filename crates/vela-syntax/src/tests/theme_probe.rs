#[test]
fn probe() {
    for src in [
        "theme dusk:\n    bg = 0x10121a\n",
        "theme dusk:\n    color bg = 0x10121a\n",
    ] {
        let parsed = vela_syntax::parse(vela_span::FileId::from_raw(0), src);
        println!("{:?} -> {:?}", src.lines().nth(1), parsed.diagnostics.iter().map(|d| d.message.clone()).collect::<Vec<_>>());
    }
}
