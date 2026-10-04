fn main() {
    let query = txtql::Query::compile("TEXT = 1 TO n LINE SPLITBY NL").unwrap();
    let out = query.run("one\ntwo\nthree", &txtql::Options::default()).unwrap();
    println!("{}", out.value); // ["one","two","three"]
}
