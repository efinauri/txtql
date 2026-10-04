//! Throughput benchmarks. `cargo bench` reports time per input size; compare sizes to check
//! that matching stays linear.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use txtql::{Options, Query};

const RHYMES: &str = "color  = WORD
colors = 1 TO n color SPLITBY (' and ' OR ', ')
rhyme  = noun:WORD ' are ' colors NL AS { noun: colors }
TEXT   = 1 TO n rhyme AS { rhyme }";

const LOGS: &str = "date  = FLOAT '-' FLOAT '-' FLOAT
time  = FLOAT ':' FLOAT ':' FLOAT
entry = d:date ' ' t:time ' [' level:WORD '] ' msg:(ANY UNTILBEFORE NL) NL
    WHERE level != 'DEBUG'
    AS { 'at': JOIN([d, t], 'T'), 'level': LOWER(level), 'message': msg }
TEXT  = 1 TO n entry SKIPPING (LINE NL) AS [ entry FOR entry ]";

fn rhymes(bytes: usize) -> String {
    let nouns = ["roses", "violets", "bees", "skies", "seas", "trees", "cats"];
    let colors = ["red", "blue and green", "black, white and grey"];
    let mut s = String::new();
    let mut i = 0;
    while s.len() < bytes {
        s.push_str(&format!("{} are {}\n", nouns[i % 7], colors[i % 3]));
        i += 1;
    }
    s
}

fn logs(bytes: usize) -> String {
    let lines = [
        "2024-01-01 12:00:01 [INFO] server started port=8080\n",
        "2024-01-01 12:00:02 [DEBUG] config loaded\n",
        "free-form noise that is skipped\n",
        "2024-01-01 12:00:05 [ERROR] connection refused host=db.local\n",
    ];
    let mut s = String::new();
    let mut i = 0;
    while s.len() < bytes {
        s.push_str(lines[i % lines.len()]);
        i += 1;
    }
    s
}

fn bench_query(c: &mut Criterion, name: &str, query: &str, make: fn(usize) -> String) {
    let q = Query::compile(query).unwrap();
    let mut group = c.benchmark_group(name);
    group.sample_size(10);
    for size in [100_000, 400_000, 1_600_000] {
        let input = make(size);
        group.throughput(Throughput::Bytes(input.len() as u64));
        for (label, check) in [("with-ambiguity-check", true), ("no-ambiguity-check", false)] {
            let opts = Options { check_ambiguity: check, ..Options::default() };
            group.bench_with_input(BenchmarkId::new(label, size), &input, |b, input| {
                b.iter(|| q.run(black_box(input), &opts).unwrap())
            });
        }
    }
    group.finish();
}

fn rhyme_bench(c: &mut Criterion) {
    bench_query(c, "rhymes", RHYMES, rhymes);
}

fn log_bench(c: &mut Criterion) {
    bench_query(c, "logs", LOGS, logs);
}

fn pathological(c: &mut Criterion) {
    let q = Query::compile("TEXT = 1 TO n p:(1 TO n DIGIT)").unwrap();
    let mut group = c.benchmark_group("nested-repetition");
    group.sample_size(10);
    for n in [100, 200, 400] {
        let input = "1".repeat(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &input, |b, input| {
            b.iter(|| q.run(black_box(input), &Options::default()))
        });
    }
    group.finish();
}

fn compile_bench(c: &mut Criterion) {
    c.bench_function("compile/logs", |b| b.iter(|| Query::compile(black_box(LOGS)).unwrap()));
}

criterion_group!(benches, rhyme_bench, log_bench, pathological, compile_bench);
criterion_main!(benches);
