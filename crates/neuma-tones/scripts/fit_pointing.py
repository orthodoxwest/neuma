#!/usr/bin/env python3
"""Fits the pointer's weights (tones/pointing.weights) to a hand-pointed psalter.

usage: cargo run -p neuma-tones --example pointing_data < halves.tsv > halves.jsonl
       python3 fit_pointing.py halves.jsonl [--l2 L] [--folds K] [--out pointing.weights]

The model is a softmax over each half-verse's candidate placements, scored by the sum of their
features' weights, with an L2 penalty. With --folds it first reports agreement on psalms held
out of the fit (folds by id before any `|`), and the temperature that calibrates the
held-out confidences. Needs numpy and scipy.
"""
import argparse, json, random
import numpy as np
from scipy.optimize import minimize


def load(path):
    """The halves with a choice to make, and how many the pointer gets right and wrong without one."""
    halves = [json.loads(l) for l in open(path, encoding='utf-8')]
    unreachable = sum(h['chosen'] is None for h in halves)
    only = sum(h['chosen'] is not None and len(h['candidates']) == 1 for h in halves)
    return [h for h in halves if h['chosen'] is not None and len(h['candidates']) > 1], only, unreachable


def index(halves):
    names = sorted({f for h in halves for c in h['candidates'] for f in c})
    return names, {f: i for i, f in enumerate(names)}


def design(halves, ix):
    """Rows of feature indices per candidate, each half's candidate range, and its choice."""
    rows, cols, starts, chosen = [], [], [0], []
    r = 0
    for h in halves:
        for c in h['candidates']:
            for f in c:
                if f in ix:
                    rows.append(r)
                    cols.append(ix[f])
            r += 1
        chosen.append(starts[-1] + h['chosen'])
        starts.append(r)
    from scipy.sparse import csr_matrix
    x = csr_matrix((np.ones(len(rows)), (rows, cols)), shape=(r, len(ix)))
    return x, np.array(starts), np.array(chosen)


def fit(halves, l2):
    names, ix = index(halves)
    x, starts, chosen = design(halves, ix)
    seg = np.repeat(np.arange(len(starts) - 1), np.diff(starts))

    def loss(w):
        s = x @ w
        m = np.maximum.reduceat(s, starts[:-1])
        e = np.exp(s - m[seg])
        z = np.add.reduceat(e, starts[:-1])
        p = e / z[seg]
        nll = -(s[chosen] - m - np.log(z)).sum()
        g = x.T @ p - np.asarray(x[chosen].sum(axis=0)).ravel()
        return nll + l2 * w @ w, g + 2 * l2 * w

    w = minimize(loss, np.zeros(len(names)), jac=True, method='L-BFGS-B', options={'maxiter': 2000}).x
    return dict(zip(names, w))


def predict(weights, h, t=1.0):
    s = np.array([sum(weights.get(f, 0.0) for f in c) for c in h['candidates']])
    best = int(np.argmax(s))
    return best, 1.0 / np.exp((s - s[best]) / t).sum()


def held_out(halves, l2, folds):
    ids = sorted({h['id'].split('|')[0] for h in halves})
    random.Random(0).shuffle(ids)
    fold = {p: i % folds for i, p in enumerate(ids)}
    results = []
    for k in range(folds):
        w = fit([h for h in halves if fold[h['id'].split('|')[0]] != k], l2)
        for h in halves:
            if fold[h['id'].split('|')[0]] == k:
                s = np.array([sum(w.get(f, 0.0) for f in c) for c in h['candidates']])
                results.append((h, s))
    return results


def report(results, t, only=0, unreachable=0):
    agree = [int(np.argmax(s)) == h['chosen'] for h, s in results]
    total = len(agree) + only + unreachable
    print(f'held out, of all {total} halves: {(sum(agree) + only) / total:.1%}')
    conf = [1.0 / np.exp((s - s.max()) / t).sum() for _, s in results]
    sure = [a for a, c in zip(agree, conf) if c >= 0.8]
    builtin = [a for (h, _), a in zip(results, agree) if h['id'].endswith('|1')]
    print(f'  of the {len(agree)} with a choice: {np.mean(agree):.1%}; built-in tones (ids ending |1) {np.mean(builtin):.1%}; '
          f'sure {len(sure) / len(agree):.1%}, agreeing {np.mean(sure):.1%}')


def temperature(results):
    def nll(t):
        return -sum((s[h['chosen']] - s.max()) / t - np.log(np.exp((s - s.max()) / t).sum()) for h, s in results)
    return min(np.arange(0.5, 3.01, 0.1), key=nll)


def main():
    a = argparse.ArgumentParser()
    a.add_argument('data')
    a.add_argument('--l2', type=float, default=0.3)
    a.add_argument('--folds', type=int, default=10, help='0 skips the held-out report and leaves the temperature at 1')
    a.add_argument('--out')
    args = a.parse_args()
    halves, only, unreachable = load(args.data)
    t = 1.0
    if args.folds:
        results = held_out(halves, args.l2, args.folds)
        t = float(temperature(results))
        report(results, t, only, unreachable)
        print(f'temperature {t:.1f}')
    if args.out:
        w = fit(halves, args.l2)
        with open(args.out, 'w', encoding='utf-8') as f:
            f.write('# Feature weights for the pointer (see src/point.rs), fitted to a hand-pointed English psalter\n')
            f.write('# by scripts/fit_pointing.py.\n')
            f.write(f'# temperature\t{t:.1f}\n')
            for k in sorted(w):
                if abs(w[k]) >= 0.005:
                    f.write(f'{k}\t{w[k]:.2f}\n')


if __name__ == '__main__':
    main()
