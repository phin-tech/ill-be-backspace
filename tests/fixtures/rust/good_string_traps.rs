fn main() {
    let a = r#"// not a comment"#;
    let b = "/* also not a comment */";
    /* outer /* nested */ still a comment */
    println!("{a}{b}");
}
