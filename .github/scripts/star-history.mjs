#!/usr/bin/env node
// Renders "stargazers over time" as two self-contained SVGs (light + dark).
//
// Uses only the GitHub REST API and the Node standard library: no third-party
// action and no third-party service ever sees this repository's token.
//
// GITHUB_TOKEN is required, not optional. GitHub restricted the stargazer
// listing in July 2026: unauthenticated requests get 401, a user token outside
// the repository's admins and collaborators gets 404, and an Actions token
// without `contents: write` gets 403. Hence this charts only its own
// repository, and its workflow must grant `contents: write` to read at all.
//
// Usage:
//   node .github/scripts/star-history.mjs [--repo owner/name] [--out dir]

import { mkdir, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";

const API = "https://api.github.com";
const PER_PAGE = 100;
// GitHub stops paginating stargazers past 40,000 entries; beyond that we
// sample pages instead of walking them all.
const MAX_PAGES = 400;
const SAMPLE_PAGES = 30;
// Points kept in the rendered path. More than this is invisible at 820px wide
// and only inflates the file.
const MAX_PLOT_POINTS = 400;

const HOUR = 3_600_000;
const DAY = 24 * HOUR;

// ---------------------------------------------------------------- palette --
//
// Surfaces are GitHub's own canvas colors so the chart sits flush in a README
// in either theme. The series hue is the validated data-viz blue: it clears
// the lightness band, the chroma floor, and >= 3:1 contrast against both
// surfaces (checked with the palette validator, not by eye).

const THEMES = {
  light: {
    surface: "#ffffff",
    textPrimary: "#1f2328",
    textSecondary: "#59636e",
    grid: "#e4e8ec",
    axis: "#d1d9e0",
    series: "#2a78d6",
  },
  dark: {
    surface: "#0d1117",
    textPrimary: "#e6edf3",
    textSecondary: "#9198a1",
    grid: "#21262d",
    axis: "#30363d",
    series: "#3987e5",
  },
};

const FONT =
  "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif";

// ------------------------------------------------------------------ fetch --

function ghHeaders(token, accept = "application/vnd.github+json") {
  const h = {
    Accept: accept,
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "star-history-renderer",
  };
  if (token) h.Authorization = `Bearer ${token}`;
  return h;
}

async function ghGet(url, token, accept) {
  for (let attempt = 0; attempt < 4; attempt++) {
    const res = await fetch(url, { headers: ghHeaders(token, accept) });
    if (res.ok) return res.json();

    const retryable =
      res.status >= 500 ||
      res.status === 429 ||
      (res.status === 403 && res.headers.get("x-ratelimit-remaining") === "0");
    if (!retryable || attempt === 3) {
      throw new Error(`GET ${url} -> ${res.status} ${await res.text()}`);
    }
    const reset = Number(res.headers.get("x-ratelimit-reset"));
    const waitMs = Number.isFinite(reset)
      ? Math.max(1000, reset * 1000 - Date.now() + 1000)
      : 2000 * 2 ** attempt;
    console.warn(`  ${res.status} on ${url}; retrying in ${Math.round(waitMs / 1000)}s`);
    await new Promise((r) => setTimeout(r, Math.min(waitMs, 60_000)));
  }
  throw new Error("unreachable");
}

/** Cumulative star series as [{ t: epochMs, c: count }], oldest first. */
async function fetchSeries(owner, name, token) {
  const meta = await ghGet(`${API}/repos/${owner}/${name}`, token);
  const total = meta.stargazers_count;
  console.log(`${owner}/${name}: ${total} stars`);
  if (total === 0) return { total, series: [], sampled: false };

  const starAccept = "application/vnd.github.star+json";
  const totalPages = Math.ceil(total / PER_PAGE);
  const pageUrl = (p) =>
    `${API}/repos/${owner}/${name}/stargazers?per_page=${PER_PAGE}&page=${p}`;

  if (totalPages <= MAX_PAGES) {
    const stamps = [];
    for (let p = 1; p <= totalPages; p++) {
      const page = await ghGet(pageUrl(p), token, starAccept);
      if (page.length === 0) break;
      for (const entry of page) {
        if (entry.starred_at) stamps.push(Date.parse(entry.starred_at));
      }
      process.stdout.write(`\r  fetched page ${p}/${totalPages}`);
    }
    process.stdout.write("\n");
    // A token that may not enumerate stargazers gets an empty listing rather
    // than an error on some paths (GraphQL does this silently). Refuse to
    // publish a flat-zero chart that would look like real data.
    if (stamps.length === 0) {
      throw new Error(
        `${owner}/${name} reports ${total} stars but the listing came back ` +
        `empty. Listing stargazers is restricted to repository admins and ` +
        `collaborators; check the token's permissions.`,
      );
    }
    stamps.sort((a, b) => a - b);
    return {
      total,
      series: stamps.map((t, i) => ({ t, c: i + 1 })),
      sampled: false,
    };
  }

  // Too many stars to walk. Sample the first entry of evenly spread pages:
  // page p's first entry is star number (p - 1) * PER_PAGE + 1.
  console.log(`  ${totalPages} pages exceeds the ${MAX_PAGES}-page API cap; sampling`);
  const step = Math.max(1, Math.floor(MAX_PAGES / SAMPLE_PAGES));
  const series = [];
  for (let p = 1; p <= MAX_PAGES; p += step) {
    const page = await ghGet(pageUrl(p), token, starAccept);
    const first = page[0];
    if (!first?.starred_at) continue;
    series.push({ t: Date.parse(first.starred_at), c: (p - 1) * PER_PAGE + 1 });
  }
  if (series.length === 0) {
    throw new Error(
      `${owner}/${name} reports ${total} stars but every sampled page was ` +
      `empty; check the token's permissions.`,
    );
  }
  // Pagination stops at star 40,000, so anything beyond that is a straight
  // line from the last sampled point to today's total. The subtitle says
  // "sampled" so the chart does not pass this off as measured detail.
  series.push({ t: Date.now(), c: total });
  series.sort((a, b) => a.t - b.t);
  return { total, series, sampled: true };
}

// ------------------------------------------------------------------ scales --

/** Axis ticks on 1/2/5 x 10^k boundaries, always starting at zero. */
function valueTicks(max, target = 5) {
  if (max <= 0) return [0, 1];
  const raw = max / target;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const norm = raw / mag;
  // Stars are whole numbers, so never step by a fraction of one.
  const step = Math.max(1, (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag);
  const ticks = [];
  for (let v = 0; v <= max - 1e-9; v += step) ticks.push(v);
  ticks.push(ticks[ticks.length - 1] + step);
  return ticks;
}

function hourTicks(t0, t1, stepHours) {
  const out = [];
  const step = stepHours * HOUR;
  for (let t = Math.ceil(t0 / step) * step; t <= t1; t += step) out.push(t);
  return out;
}

function monthTicks(t0, t1, stepMonths) {
  const out = [];
  const d = new Date(t0);
  let y = d.getUTCFullYear();
  let m = Math.ceil(d.getUTCMonth() / stepMonths) * stepMonths;
  while (m >= 12) { m -= 12; y += 1; }
  for (;;) {
    const t = Date.UTC(y, m, 1);
    if (t > t1) break;
    if (t >= t0) out.push(t);
    m += stepMonths;
    while (m >= 12) { m -= 12; y += 1; }
  }
  return out;
}

function dayTicks(t0, t1, stepDays) {
  const out = [];
  const start = Math.ceil(t0 / DAY) * DAY;
  for (let t = start; t <= t1; t += stepDays * DAY) out.push(t);
  return out;
}

/** Calendar-aligned time ticks, roughly `target` of them. */
function timeTicks(t0, t1, target = 5) {
  const hours = (t1 - t0) / HOUR;
  if (!(hours > 0)) return [t0, t1];
  for (const s of [1, 2, 3, 6, 12]) {
    if (hours / s <= target + 1) return hourTicks(t0, t1, s);
  }
  const days = hours / 24;
  for (const s of [1, 2, 3, 7, 14]) {
    if (days / s <= target + 1) return dayTicks(t0, t1, s);
  }
  for (const s of [1, 2, 3, 6, 12, 24, 60, 120, 240]) {
    if (days / (s * 30.44) <= target + 1) return monthTicks(t0, t1, s);
  }
  return monthTicks(t0, t1, 240);
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function fmtTick(t, spanMs) {
  const d = new Date(t);
  if (spanMs <= 3 * DAY) {
    return `${String(d.getUTCHours()).padStart(2, "0")}:` +
           `${String(d.getUTCMinutes()).padStart(2, "0")}`;
  }
  if (spanMs <= 90 * DAY) return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}`;
  if (spanMs <= 3 * 365 * DAY) return `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
  return String(d.getUTCFullYear());
}

const fmtNum = (n) => n.toLocaleString("en-US");
const isoDay = (t) => new Date(t).toISOString().slice(0, 10);
// Rough advance width for the label-collision checks below. Deliberately
// generous: over-estimating drops a tick, under-estimating overlaps text.
const textWidth = (s, size) => s.length * size * 0.58;
const esc = (s) =>
  String(s).replace(/[&<>"]/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

/** Even-index sampling that always keeps the first and last point. */
function downsample(series, max) {
  if (series.length <= max) return series;
  const out = [];
  const stride = (series.length - 1) / (max - 1);
  for (let i = 0; i < max; i++) out.push(series[Math.round(i * stride)]);
  out[out.length - 1] = series[series.length - 1];
  return out;
}

// ------------------------------------------------------------------ render --

const W = 820;
const H = 420;
const PAD = { top: 68, right: 84, bottom: 46, left: 64 };

function renderSvg({ slug, total, series, sampled, mode }) {
  const c = THEMES[mode];
  const open =
    `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" ` +
    `viewBox="0 0 ${W} ${H}" role="img" ` +
    `aria-label="${esc(slug)} stargazers over time: ${fmtNum(total)} stars">`;
  const bg = `<rect width="${W}" height="${H}" fill="${c.surface}"/>`;

  const head =
    `<text x="${PAD.left}" y="34" font-family="${FONT}" font-size="15" ` +
    `font-weight="600" fill="${c.textPrimary}">Stargazers over time</text>` +
    `<text x="${PAD.left}" y="52" font-family="${FONT}" font-size="12" ` +
    `fill="${c.textSecondary}">${esc(slug)} · updated ${isoDay(Date.now())}` +
    `${sampled ? " · sampled" : ""}</text>`;

  if (series.length === 0) {
    return [
      open, bg, head,
      `<text x="${W / 2}" y="${H / 2 + 10}" text-anchor="middle" ` +
      `font-family="${FONT}" font-size="13" fill="${c.textSecondary}">` +
      `No stars yet</text>`,
      "</svg>",
    ].join("");
  }

  const now = Date.now();
  const pts = downsample(series, MAX_PLOT_POINTS).slice();
  // Anchor the line on the baseline at the first star (the count really was
  // zero right up to that instant) and hold the final count flat through now,
  // so the chart reads "as of today" rather than stopping at the last star.
  pts.unshift({ t: pts[0].t, c: 0 });
  if (now > pts[pts.length - 1].t) pts.push({ t: now, c: total });

  const t0 = pts[0].t;
  // Never let the domain collapse to a point: a repo whose stars all landed
  // within the same minute still needs an axis to hang labels on.
  const t1 = Math.max(pts[pts.length - 1].t, t0 + HOUR);
  const span = t1 - t0;
  const yTicks = valueTicks(total);
  const yMax = yTicks[yTicks.length - 1];

  const plotL = PAD.left;
  const plotR = W - PAD.right;
  const plotT = PAD.top;
  const plotB = H - PAD.bottom;
  const x = (t) => plotL + ((t - t0) / span) * (plotR - plotL);
  const y = (v) => plotB - (v / yMax) * (plotB - plotT);

  const parts = [open, bg, head];

  // Gridlines and value labels. Hairline, solid, one step off the surface.
  for (const v of yTicks) {
    const yy = y(v).toFixed(1);
    parts.push(
      `<line x1="${plotL}" y1="${yy}" x2="${plotR}" y2="${yy}" ` +
      `stroke="${v === 0 ? c.axis : c.grid}" stroke-width="1"/>`,
      `<text x="${plotL - 10}" y="${(y(v) + 4).toFixed(1)}" text-anchor="end" ` +
      `font-family="${FONT}" font-size="11" fill="${c.textSecondary}">${fmtNum(v)}</text>`,
    );
  }

  // Time labels, thinned until they stop colliding.
  let xt = timeTicks(t0, t1);
  // A very short history can land on a single calendar boundary; an axis
  // with one label reads as broken, so fall back to the domain ends.
  if (xt.length < 2) xt = [t0, t1];
  const labels = xt.map((t) => fmtTick(t, span));
  const widest = Math.max(...labels.map((l) => textWidth(l, 11)));
  while (xt.length > 2 && (widest + 16) * xt.length > plotR - plotL) {
    xt = xt.filter((_, i) => i % 2 === 0);
  }
  for (const t of xt) {
    const xx = x(t);
    // Keep the first and last label from hanging off the plot edges.
    const anchor = xx - plotL < 20 ? "start" : plotR - xx < 20 ? "end" : "middle";
    parts.push(
      `<text x="${xx.toFixed(1)}" y="${plotB + 20}" text-anchor="${anchor}" ` +
      `font-family="${FONT}" font-size="11" fill="${c.textSecondary}">` +
      `${esc(fmtTick(t, span))}</text>`,
    );
  }

  // Area wash under the line, then the line itself.
  const coords = pts.map((p) => `${x(p.t).toFixed(1)},${y(p.c).toFixed(1)}`);
  const poly = coords.join(" ");
  const area =
    `M${x(t0).toFixed(1)},${plotB} L${coords.join(" L")} L${x(t1).toFixed(1)},${plotB} Z`;
  parts.push(
    `<path d="${area}" fill="${c.series}" fill-opacity="0.1"/>`,
    `<polyline points="${poly}" fill="none" stroke="${c.series}" ` +
    `stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>`,
  );

  // Endpoint marker with a surface ring, plus the one direct label the chart
  // gets: the current total. There is no tooltip layer to fall back on, since
  // GitHub serves README images through a proxy that strips interactivity.
  const ex = x(t1);
  const ey = y(total);
  parts.push(
    `<circle cx="${ex.toFixed(1)}" cy="${ey.toFixed(1)}" r="4.5" ` +
    `fill="${c.series}" stroke="${c.surface}" stroke-width="2"/>`,
  );

  const label = fmtNum(total);
  const labelW = textWidth(label, 13);
  const fitsRight = ex + 12 + labelW <= W - 8;
  parts.push(
    `<text x="${(fitsRight ? ex + 12 : ex - 12).toFixed(1)}" ` +
    `y="${(ey + 4.5).toFixed(1)}" text-anchor="${fitsRight ? "start" : "end"}" ` +
    `font-family="${FONT}" font-size="13" font-weight="600" ` +
    `fill="${c.textPrimary}">${label}</text>`,
  );

  parts.push("</svg>");
  return parts.join("");
}

// -------------------------------------------------------------------- main --

function parseArgs(argv) {
  const out = { out: "dist" };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--repo") out.repo = argv[++i];
    else if (argv[i] === "--out") out.out = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return out;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const slug = args.repo || process.env.GITHUB_REPOSITORY;
  if (!slug || !slug.includes("/")) {
    throw new Error("pass --repo owner/name or set GITHUB_REPOSITORY");
  }
  const [owner, name] = slug.split("/");

  const { total, series, sampled } = await fetchSeries(
    owner, name, process.env.GITHUB_TOKEN,
  );
  if (series.length > 0) {
    console.log(`  first star ${isoDay(series[0].t)}, ${series.length} data points`);
  }

  await mkdir(args.out, { recursive: true });
  for (const mode of ["light", "dark"]) {
    const file = `${args.out}/star-history${mode === "dark" ? "-dark" : ""}.svg`;
    await writeFile(file, renderSvg({ slug, total, series, sampled, mode }));
    console.log(`  wrote ${file}`);
  }
}

export { renderSvg, valueTicks, timeTicks, downsample };

const invokedDirectly =
  process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;
if (invokedDirectly) {
  main().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
