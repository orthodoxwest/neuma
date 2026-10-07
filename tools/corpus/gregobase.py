"""Extracts the chants of a GregoBase SQL dump as .gabc files, for the nightly corpus run.

usage: python3 -I tools/corpus/gregobase.py <gregobase_online.sql> <outdir>

The dump is `gregobase_online.sql` from https://github.com/gregorio-project/GregoBase,
which the corpus workflow fetches at a pinned commit. Each file is what GregoBase's own
download page serves: a `name:` and `mode:` header when the score has none, then the GABC,
then any verses. Chants GregoBase marks as still under copyright are skipped. Nothing here
is kept or published: the files are a scratch input for one run.
"""
import json
import os
import sys

ESC = {'0': '\0', 'n': '\n', 'r': '\r', 't': '\t', 'Z': '\x1a', 'b': '\b'}


def read_string(s, i):
    """Reads the SQL string literal starting at s[i] (a quote); returns (text, next index)."""
    i += 1
    buf = []
    while True:
        c = s[i]
        if c == '\\':
            buf.append(ESC.get(s[i + 1], s[i + 1]))
            i += 2
        elif c == "'":
            if s[i + 1] == "'":
                buf.append("'")
                i += 2
            else:
                return ''.join(buf), i + 1
        else:
            buf.append(c)
            i += 1


def read_rows(s, i, rows):
    """Reads `(v, ...), (...);` starting at s[i] into rows; returns the index after `;`."""
    while True:
        while s[i] in ' \n\r\t,':
            i += 1
        if s[i] == ';':
            return i + 1
        if s[i] != '(':
            raise ValueError('unexpected %r at %d' % (s[i:i + 40], i))
        i += 1
        row = []
        while True:
            while s[i] in ' \n\r\t':
                i += 1
            if s[i] == "'":
                value, i = read_string(s, i)
            else:
                j = i
                while s[j] not in ',)':
                    j += 1
                token = s[i:j].strip()
                value = None if token == 'NULL' else token
                i = j
            row.append(value)
            while s[i] in ' \n\r\t':
                i += 1
            i += 1
            if s[i - 1] == ')':
                break
        rows.append(row)


def chants(sql):
    marker = 'INSERT INTO `gregobase_chants` ('
    pos = 0
    while True:
        k = sql.find(marker, pos)
        if k < 0:
            return
        e = sql.index(') VALUES', k)
        cols = [c.strip(' `') for c in sql[k + len(marker):e].split(',')]
        rows = []
        pos = read_rows(sql, e + len(') VALUES'), rows)
        for r in rows:
            yield dict(zip(cols, r))


def bodies(raw):
    """The GABC bodies of a chant's `gabc` field: a JSON string or a list of parts."""
    try:
        content = json.loads(raw)
    except ValueError:
        content = raw
    if isinstance(content, str):
        return [content]
    if isinstance(content, list):
        return [e[1] for e in content if isinstance(e, list) and len(e) > 1 and e[0] == 'gabc']
    return []


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    with open(sys.argv[1], encoding='utf-8') as f:
        sql = f.read()
    out = sys.argv[2]
    os.makedirs(out, exist_ok=True)
    written = copyrighted = empty = 0
    for c in chants(sql):
        if c.get('copyrighted') not in (None, '0'):
            copyrighted += 1
            continue
        parts = [b for b in bodies(c.get('gabc') or '') if b and b.strip()]
        if not parts:
            empty += 1
            continue
        for n, body in enumerate(parts):
            head = ''
            if '%%' not in body:
                head = 'name:%s;\n' % (c.get('incipit') or '').replace(';', ',')
                if c.get('mode'):
                    head += 'mode:%s;\n' % c['mode']
                head += '%%\n'
            text = head + body
            if c.get('gabc_verses') and len(parts) == 1:
                text += '\n' + c['gabc_verses']
            suffix = '' if len(parts) == 1 else '.%d' % (n + 1)
            name = '%s%s.gabc' % (int(c['id']), suffix)
            with open(os.path.join(out, name), 'w', encoding='utf-8') as f:
                f.write(text)
            written += 1
    print('%d files written; skipped %d chants marked copyrighted and %d without GABC'
          % (written, copyrighted, empty))


main()
