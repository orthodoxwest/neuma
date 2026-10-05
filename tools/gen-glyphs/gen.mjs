// Converts exsurge's Glyphs.js (MIT, (c) 2008-2016 Fr. Matthew Spencer, OSJ) into
// crates/neuma/src/glyphs/table.rs, normalizing every outline to absolute M/L/C/Z with holes
// as reverse-wound subpaths, so a nonzero fill draws them and simple path parsers (the Office's
// iOS SVG.swift handles only M L H V C A Z) can read every glyph.
//
// Usage: node tools/gen-glyphs/gen.mjs path/to/Exsurge.Glyphs.js > crates/neuma/src/glyphs/table.rs
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const srcPath = process.argv[2];
const src = readFileSync(srcPath, "utf8");
const Glyphs = new Function(src.replace(/export let Glyphs =/, "return"))();
const sourceHash = createHash("sha256").update(src).digest("hex");

// ---- path parsing -------------------------------------------------------------------------

function tokenize(d) {
  const re = /([MmLlHhVvCcSsQqTtAaZz])|(-?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)/g;
  const out = [];
  let m;
  while ((m = re.exec(d))) out.push(m[1] ? { cmd: m[1] } : { num: parseFloat(m[2]) });
  return out;
}

// Arc flags may be written without separators ("a5 5 0 011 1"); re-tokenize arcs carefully.
function parse(d) {
  const toks = tokenize(d);
  const segs = [];
  let i = 0;
  let cmd = null;
  const nums = (n) => {
    const r = [];
    for (let k = 0; k < n; k++) {
      if (i >= toks.length || toks[i].cmd) throw new Error(`path ended early in ${cmd}: ${d.slice(0, 60)}`);
      r.push(toks[i++].num);
    }
    return r;
  };
  const arity = { M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, A: 7, Z: 0 };
  while (i < toks.length) {
    if (toks[i].cmd) cmd = toks[i++].cmd;
    else if (!cmd) throw new Error("path starts with a number");
    const up = cmd.toUpperCase();
    segs.push({ cmd, args: nums(arity[up]) });
    if (up === "M") cmd = cmd === "M" ? "L" : "l"; // further pairs after a move are lines
    if (up === "Z") cmd = null;
  }
  return segs;
}

// ---- normalization to absolute M/L/C/Z ------------------------------------------------------

function arcToCubics(x1, y1, rx, ry, phiDeg, fa, fs, x2, y2) {
  if (rx === 0 || ry === 0) return [[x1, y1, x2, y2, x2, y2]];
  const phi = (phiDeg * Math.PI) / 180;
  const cos = Math.cos(phi), sin = Math.sin(phi);
  const dx = (x1 - x2) / 2, dy = (y1 - y2) / 2;
  const x1p = cos * dx + sin * dy, y1p = -sin * dx + cos * dy;
  rx = Math.abs(rx); ry = Math.abs(ry);
  const lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
  if (lambda > 1) { rx *= Math.sqrt(lambda); ry *= Math.sqrt(lambda); }
  const sign = fa === fs ? -1 : 1;
  const num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
  const den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
  const coef = sign * Math.sqrt(Math.max(0, num / den));
  const cxp = (coef * rx * y1p) / ry, cyp = (-coef * ry * x1p) / rx;
  const cx = cos * cxp - sin * cyp + (x1 + x2) / 2, cy = sin * cxp + cos * cyp + (y1 + y2) / 2;
  const ang = (ux, uy, vx, vy) => {
    const a = Math.atan2(ux * vy - uy * vx, ux * vx + uy * vy);
    return a;
  };
  const t1 = ang(1, 0, (x1p - cxp) / rx, (y1p - cyp) / ry);
  let dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
  if (!fs && dt > 0) dt -= 2 * Math.PI;
  if (fs && dt < 0) dt += 2 * Math.PI;
  const n = Math.max(1, Math.ceil(Math.abs(dt) / (Math.PI / 2)));
  const step = dt / n;
  const k = (4 / 3) * Math.tan(step / 4);
  const out = [];
  let t = t1;
  const pt = (a) => [cx + rx * Math.cos(a) * cos - ry * Math.sin(a) * sin, cy + rx * Math.cos(a) * sin + ry * Math.sin(a) * cos];
  const deriv = (a) => [-rx * Math.sin(a) * cos - ry * Math.cos(a) * sin, -rx * Math.sin(a) * sin + ry * Math.cos(a) * cos];
  for (let s = 0; s < n; s++) {
    const a0 = t, a1 = t + step;
    const [px0, py0] = pt(a0), [px1, py1] = pt(a1);
    const [dx0, dy0] = deriv(a0), [dx1, dy1] = deriv(a1);
    out.push([px0 + k * dx0, py0 + k * dy0, px1 - k * dx1, py1 - k * dy1, s === n - 1 ? x2 : px1, s === n - 1 ? y2 : py1]);
    t = a1;
  }
  return out;
}

// Returns subpaths: [{ start: [x, y], segs: [{ type: "L"|"C", pts: [...] }] }]
function normalize(d) {
  const subpaths = [];
  let cur = null;
  let x = 0, y = 0, sx = 0, sy = 0;
  let lastCtrl = null; // reflected control point for S
  let lastQ = null; // control point for T
  for (const { cmd, args } of parse(d)) {
    const rel = cmd === cmd.toLowerCase();
    const up = cmd.toUpperCase();
    const ox = rel ? x : 0, oy = rel ? y : 0;
    let ctrl = null, q = null;
    const ensure = () => {
      if (!cur) { cur = { start: [x, y], segs: [] }; subpaths.push(cur); }
    };
    switch (up) {
      case "M":
        x = ox + args[0]; y = oy + args[1]; sx = x; sy = y;
        cur = { start: [x, y], segs: [] };
        subpaths.push(cur);
        break;
      case "L": ensure(); x = ox + args[0]; y = oy + args[1]; cur.segs.push({ type: "L", pts: [x, y] }); break;
      case "H": ensure(); x = (rel ? x : 0) + args[0]; cur.segs.push({ type: "L", pts: [x, y] }); break;
      case "V": ensure(); y = (rel ? y : 0) + args[0]; cur.segs.push({ type: "L", pts: [x, y] }); break;
      case "C": {
        ensure();
        const p = [ox + args[0], oy + args[1], ox + args[2], oy + args[3], ox + args[4], oy + args[5]];
        cur.segs.push({ type: "C", pts: p });
        ctrl = [p[2], p[3]]; x = p[4]; y = p[5];
        break;
      }
      case "S": {
        ensure();
        const c1 = lastCtrl ? [2 * x - lastCtrl[0], 2 * y - lastCtrl[1]] : [x, y];
        const p = [c1[0], c1[1], ox + args[0], oy + args[1], ox + args[2], oy + args[3]];
        cur.segs.push({ type: "C", pts: p });
        ctrl = [p[2], p[3]]; x = p[4]; y = p[5];
        break;
      }
      case "Q":
      case "T": {
        ensure();
        let qx, qy, ex, ey;
        if (up === "Q") { qx = ox + args[0]; qy = oy + args[1]; ex = ox + args[2]; ey = oy + args[3]; }
        else {
          [qx, qy] = lastQ ? [2 * x - lastQ[0], 2 * y - lastQ[1]] : [x, y];
          ex = ox + args[0]; ey = oy + args[1];
        }
        cur.segs.push({ type: "C", pts: [x + (2 / 3) * (qx - x), y + (2 / 3) * (qy - y), ex + (2 / 3) * (qx - ex), ey + (2 / 3) * (qy - ey), ex, ey] });
        q = [qx, qy]; x = ex; y = ey;
        break;
      }
      case "A": {
        ensure();
        const ex = ox + args[5], ey = oy + args[6];
        for (const c of arcToCubics(x, y, args[0], args[1], args[2], args[3] !== 0, args[4] !== 0, ex, ey))
          cur.segs.push({ type: "C", pts: c });
        x = ex; y = ey;
        break;
      }
      case "Z":
        if (cur) { cur.closed = true; x = sx; y = sy; cur = null; }
        break;
    }
    lastCtrl = ctrl;
    lastQ = q;
  }
  return subpaths.filter((s) => s.segs.length > 0);
}

// Signed area by sampling curve endpoints and control polygon (sign is all we need).
function signedArea(sp) {
  let a = 0;
  let [px, py] = sp.start;
  for (const s of sp.segs) {
    const pts = s.type === "L" ? [s.pts] : [s.pts.slice(0, 2), s.pts.slice(2, 4), s.pts.slice(4, 6)];
    for (const [qx, qy] of pts) { a += px * qy - qx * py; px = qx; py = qy; }
  }
  a += px * sp.start[1] - sp.start[0] * py;
  return a / 2;
}

function reverse(sp) {
  const nodes = [sp.start];
  for (const s of sp.segs) nodes.push(s.type === "L" ? s.pts : s.pts.slice(4, 6));
  const segs = [];
  for (let k = sp.segs.length - 1; k >= 0; k--) {
    const s = sp.segs[k];
    const to = nodes[k];
    if (s.type === "L") segs.push({ type: "L", pts: to });
    else segs.push({ type: "C", pts: [s.pts[2], s.pts[3], s.pts[0], s.pts[1], to[0], to[1]] });
  }
  return { start: nodes[nodes.length - 1], segs, closed: true };
}

const fmt = (v) => {
  let s = (Math.round(v * 1000) / 1000).toFixed(3).replace(/\.?0+$/, "");
  if (s === "-0") s = "0";
  return s;
};
const write = (subpaths) =>
  subpaths
    .map((sp) => `M${fmt(sp.start[0])} ${fmt(sp.start[1])}` + sp.segs.map((s) => s.type + s.pts.map(fmt).join(" ")).join("") + "Z")
    .join("");

// ---- emit ----------------------------------------------------------------------------------

const names = Object.keys(Glyphs).filter((n) => n !== "None");
const num = (v) => { const s = fmt(+v); return s.includes(".") ? s : s + ".0"; };
let out = `// @generated by tools/gen-glyphs/gen.mjs from exsurge's Exsurge.Glyphs.js
// (sha256 ${sourceHash}).
// Glyph outlines (c) 2008-2016 Fr. Matthew Spencer, OSJ, drawn from his Caeciliae font and used
// under the MIT license (see NOTICE). Outlines are normalized to absolute M/L/C/Z; holes are
// reverse-wound subpaths for a nonzero fill. Do not edit by hand.

use super::{Align, GlyphData};

/// The source file's hash, so a regeneration shows up in review.
pub const SOURCE_SHA256: &str = "${sourceHash}";

/// A glyph in the outline table. Its discriminant is its stable \`u16\` id across bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum GlyphId {
${names.map((n, i) => `    ${n} = ${i},`).join("\n")}
}

impl GlyphId {
    /// Every glyph, in id order.
    pub const ALL: [GlyphId; ${names.length}] = [${names.map((n) => `GlyphId::${n}`).join(", ")}];

    /// The glyph's name, as exsurge spells it.
    pub const fn name(self) -> &'static str {
        match self {
${names.map((n) => `            GlyphId::${n} => "${n}",`).join("\n")}
        }
    }
}

pub(super) static TABLE: [GlyphData; ${names.length}] = [
`;
for (const n of names) {
  const g = Glyphs[n];
  const pos = g.paths.filter((p) => p.type === "positive").flatMap((p) => normalize(p.data));
  const neg = g.paths.filter((p) => p.type === "negative").flatMap((p) => normalize(p.data));
  const outerSign = Math.sign(pos.reduce((a, sp) => a + signedArea(sp), 0)) || 1;
  const holes = neg.map((sp) => (Math.sign(signedArea(sp)) === outerSign ? reverse(sp) : sp));
  const d = write([...pos, ...holes]);
  const subpaths = pos.length + holes.length;
  // Ink box from the outline's points (control points included, so it never undershoots).
  const pts = [...pos, ...holes].flatMap((sp) => [sp.start, ...sp.segs.flatMap((sg) => {
    const out = [];
    for (let i = 0; i < sg.pts.length; i += 2) out.push([sg.pts[i], sg.pts[i + 1]]);
    return out;
  })]);
  const ink = [
    Math.min(...pts.map((q) => q[0])), Math.min(...pts.map((q) => q[1])),
    Math.max(...pts.map((q) => q[0])), Math.max(...pts.map((q) => q[1])),
  ];
  out += `    GlyphData {
        d: ${JSON.stringify(d)},
        subpaths: ${subpaths},
        width: ${num(g.bounds.width)},
        height: ${num(g.bounds.height)},
        origin_x: ${num(g.origin.x)},
        origin_y: ${num(g.origin.y)},
        align: Align::${g.align === "right" ? "Right" : "Left"},
        ink: [${ink.map(num).join(", ")}],
    },
`;
}
out += "];\n";
process.stdout.write(out);
