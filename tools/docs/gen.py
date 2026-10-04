#!/usr/bin/env python3
"""Regenerates README.md and wiki/*.md from templates in ./src, running every {{ex NAME}} example
against target/release/txtql. `gen.py --check` regenerates into memory and diffs against the repo
(fails if any doc is stale or any example behaves differently from what is documented).
Examples may declare `expect` (exit code); a mismatch is reported as FAILED."""
import os, re, subprocess, sys, tempfile, pathlib
HERE = pathlib.Path(__file__).parent
REPO = HERE.parent.parent
BIN = REPO / 'target/release/txtql'
sys.path.insert(0, str(HERE))
from examples import EXAMPLES

cache = {}
failed = []
def run(name):
    if name in cache: return cache[name]
    ex = EXAMPLES[name]
    with tempfile.TemporaryDirectory() as d:
        pathlib.Path(d, 'query.tql').write_text(ex['q'])
        pathlib.Path(d, 'input.txt').write_bytes(ex['i'].encode())
        args = [str(BIN)]
        mode = ex.get('mode', 'run')
        if mode == 'check': args.append('check')
        args += ex.get('flags', [])
        if ex.get('via') == 'expr':
            args += ['-e', ex['q']]
            args += ['input.txt'] if ex.get('file') else []
        else:
            args += ['query.tql', 'input.txt']
        p = subprocess.run(args, cwd=d, capture_output=True, text=True, input=ex['i'] if ex.get('via') == 'expr' else '', env={**os.environ, 'NO_COLOR': '1'})
    exp = ex.get('expect', 0)
    if p.returncode != exp:
        failed.append(f'{name}: exit {p.returncode}, expected {exp}')
    strip = lambda t: '\n'.join(l.rstrip() for l in t.rstrip().split('\n'))
    res = (p.returncode, strip(p.stdout), strip(p.stderr))
    cache[name] = res
    return res

def fence(lang, text):
    return f'```{lang}\n{text}\n```'

def render(name, variant=''):
    ex = EXAMPLES[name]
    code, out, err = run(name)
    parts = []
    mode = ex.get('mode', 'run')
    flags = ex.get('flags', [])
    cmd = 'txtql ' + ('check ' if mode == 'check' else '') + ' '.join(flags + ['query.tql', 'input.txt']).replace('  ', ' ')
    parts.append('**Query** (`query.tql`)\n\n' + fence('txtql', ex['q'].rstrip('\n')))
    if ex.get('via') == 'expr':
        import shlex
        inp = ex['i'].replace('\\', '\\\\').replace('\n', '\\n').replace('\t', '\\t')
        qq = ex['q'].rstrip('\n')
        qs = '"' + qq + '"' if not any(c in qq for c in '"$`\\!') else shlex.quote(qq)
        sh = "printf '" + inp + "' | txtql " + ' '.join(flags + ['-e', qs])
        res = '**Command**\n\n' + fence('sh', sh)
        if code == 0:
            res += '\n\n**Output**\n\n' + fence('json', out)
            if err: res += '\n\n**Warnings** (standard error)\n\n' + fence('text', err)
        else:
            res += f'\n\n**Error** (standard error, exit code {code})\n\n' + fence('text', err)
        return res
    if not variant == 'noinput':
        shown = ex['i']
        if shown.endswith('\n'): shown = shown[:-1]
        note = ', ends with a line break' if ex['i'].endswith('\n') else ''
        parts.append(f'**Input** (`input.txt`{note})\n\n' + fence('text', shown))
    if flags or mode == 'check':
        parts.append(f'**Command**\n\n```\n{cmd}\n```')
    if mode == 'check':
        parts.append(f'**Output** (standard error, exit code {code})\n\n' + fence('text', err))
    elif code == 0:
        parts.append('**Output**\n\n' + fence('json', out))
        if err:
            parts.append('**Warnings** (standard error)\n\n' + fence('text', err))
    else:
        parts.append(f'**Error** (standard error, exit code {code})\n\n' + fence('text', err))
    return '\n\n'.join(parts)

def mini_table(group):
    from examples import MINI
    rows = ['| Pattern or feature | Query | Input | Result |', '|---|---|---|---|']
    for m in MINI[group]:
        with tempfile.TemporaryDirectory() as d:
            pathlib.Path(d, 'query.tql').write_text(m['q'])
            raw = m['i'].encode().decode('unicode_escape').encode('latin-1') if '\\' in m['i'] else m['i'].encode()
            pathlib.Path(d, 'input.txt').write_bytes(raw)
            p = subprocess.run([str(BIN), '--compact'] + m['flags'] + ['query.tql', 'input.txt'], cwd=d, capture_output=True, text=True, input='')
        if p.returncode == 0:
            res = '`' + p.stdout.strip().replace('|', '\\|') + '`'
        else:
            res = 'no match' if 'no_parse' in p.stderr else ('error: ' + p.stderr.split()[0])
        inp = '`' + m['i'].replace('|','\\|') + '`' if m['i'] else '(empty)'
        q = '`' + m['q'].replace('|','\\|') + '`'
        rows.append(f"| {m['label']} | {q} | {inp} | {res} |")
        MINI_RUN.append(m['label'])
    return '\n'.join(rows)

MINI_RUN = []
def codes_table():
    from examples import CODES
    rows = ['| Code | Meaning | Example query |', '|---|---|---|']
    for c, d in CODES.items():
        q = d['q'].replace('\n', ' / ')
        extra = ' (' + ' '.join(d['flags']) + ')' if d['flags'] and d['flags'] != ['--nofile'] else ''
        cell = d['shown'] or f"`{q.replace('|', chr(92)+'|')}`{extra}"
        rows.append(f"| `{c}` | {d['meaning'].replace('|', chr(92)+'|')} | {cell} |")
    return '\n'.join(rows)

def run_codes():
    from examples import CODES
    n = 0
    for c, d in CODES.items():
        with tempfile.TemporaryDirectory() as t:
            pathlib.Path(t, 'query.tql').write_text(d['q'])
            pathlib.Path(t, 'input.txt').write_bytes(d['binput'] if d['binput'] else d['i'].encode())
            args = [str(BIN)] + (['check'] if d['mode'] == 'check' else []) + [f for f in d['flags'] if f != '--nofile'] + ['query.tql', 'missing.txt' if '--nofile' in d['flags'] else 'input.txt']
            p = subprocess.run(args, cwd=t, capture_output=True, text=True, input='')
        n += 1
        if c not in p.stderr: failed.append(f'code {c} not reported')
    return n

def piece(kind, name):
    ex = EXAMPLES[name]
    code, out, err = run(name)
    if kind == 'q': return fence('txtql', ex['q'].rstrip('\n'))
    if kind == 'i': return fence('text', ex['i'][:-1] if ex['i'].endswith('\n') else ex['i'])
    if kind == 'o': return fence('json', out)
    if kind == 'e': return fence('text', err)
    raise SystemExit(kind)

def run_checks():
    from examples import CHECKS
    bad = 0
    for c in CHECKS:
        with tempfile.TemporaryDirectory() as t:
            pathlib.Path(t, 'query.tql').write_text(c['q'])
            pathlib.Path(t, 'input.txt').write_text(c['i'])
            p = subprocess.run([str(BIN), '--compact'] + c['flags'] + ['query.tql', 'input.txt'], cwd=t, capture_output=True, text=True, input='')
        e = c['expected']
        if e.startswith('ERR:'): ok = e[4:] in p.stderr
        elif e == 'OK': ok = p.returncode == 0
        else: ok = p.stdout.strip() == e
        if not ok:
            bad += 1
            failed.append(f"claim: {c['claim']}: got exit {p.returncode} stdout {p.stdout.strip()[:80]!r} stderr {p.stderr.strip().splitlines()[:1]}")
    return len(CHECKS)

def expand(text):
    text = re.sub(r'\{\{([qioe]) (\w+)\}\}', lambda m: piece(m.group(1), m.group(2)), text)
    text = re.sub(r'\{\{mini (\w+)\}\}', lambda m: mini_table(m.group(1)), text)
    text = text.replace('{{codes}}', codes_table())
    def sub(m):
        bits = m.group(1).split()
        return render(bits[0], bits[1] if len(bits) > 1 else '')
    return re.sub(r'\{\{ex ([^}]+)\}\}', sub, text)

def main():
    check = '--check' in sys.argv
    stale = []
    for n in EXAMPLES: run(n)
    for src in sorted((HERE / 'src').rglob('*.md')):
        rel = src.relative_to(HERE / 'src')
        out = expand(src.read_text())
        dst = REPO / rel
        if check:
            if not dst.exists() or dst.read_text() != out: stale.append(str(rel))
        else:
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_text(out)
    used = set()
    for src in (HERE / 'src').rglob('*.md'):
        used |= set(m.split()[0] for m in re.findall(r'\{\{ex ([^}]+)\}\}', src.read_text()))
        used |= set(re.findall(r'\{\{[qioe] (\w+)\}\}', src.read_text()))
    nchecks = run_checks()
    print(f'prose claims checked: {nchecks}')
    ncodes = run_codes()
    print(f'error codes triggered: {ncodes}, mini-table rows run: {len(MINI_RUN)}')
    unused = set(EXAMPLES) - used
    print(f'examples defined: {len(EXAMPLES)}, run: {len(cache)}, unused: {sorted(unused)}')
    print(f'unexpected exit codes: {len(failed)}')
    for f in failed: print('  FAILED', f)
    if check: print('stale docs:', stale or 'none')
    sys.exit(1 if failed or stale else 0)
main()
