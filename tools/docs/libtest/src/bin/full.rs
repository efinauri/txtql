fn main() {
    let src = "TEXT = 1 TO n LINE SPLITBY NL";
    match txtql::Query::compile(src) {
        Ok(query) => match query.run("one\ntwo\nthree", &txtql::Options::default()) {
            Ok(out) => println!("{}", out.value), // ["one","two","three"]
            Err(e) => {
                for r in e.reports("query.tql", src, "input", "one\ntwo\nthree") {
                    eprintln!("{r:?}");
                }
            }
        },
        Err(e) => {
            for r in e.reports("query.tql", src) {
                eprintln!("{r:?}");
            }
        }
    }
}
