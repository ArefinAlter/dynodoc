// GENERATED from apps/desktop/client/src by client/build.mjs. Do not edit; edit the sources and rebuild.
"use strict";
(() => {
  // client/src/dom.ts
  function append(parent, child) {
    if (!(child == null || child === !1)) {
      if (Array.isArray(child)) {
        for (let c of child) append(parent, c);
        return;
      }
      if (typeof child == "string" || typeof child == "number") {
        parent.appendChild(document.createTextNode(String(child)));
        return;
      }
      parent.appendChild(child);
    }
  }
  function apply(el, props) {
    if (props)
      for (let key of Object.keys(props)) {
        let value = props[key];
        if (!(value == null || value === !1))
          if (key.startsWith("on") && typeof value == "function")
            el.addEventListener(key.slice(2).toLowerCase(), value);
          else if (key === "class")
            el.setAttribute("class", String(value));
          else if (key === "text")
            el.textContent = String(value);
          else if (key === "css" && typeof value == "object") {
            let style = el.style, map = value;
            for (let name2 of Object.keys(map)) style.setProperty(name2, map[name2]);
          } else key === "value" || key === "checked" || key === "disabled" || key === "hidden" || key === "indeterminate" || key === "selected" ? el[key] = value : key === "ref" && typeof value == "function" ? value(el) : value === !0 ? el.setAttribute(key, "") : el.setAttribute(key, String(value));
      }
  }
  function h(tag, props, ...children) {
    let el = document.createElement(tag);
    apply(el, props);
    for (let c of children) append(el, c);
    return el;
  }
  var SVG = "http://www.w3.org/2000/svg";
  function s(tag, props, ...children) {
    let el = document.createElementNS(SVG, tag);
    apply(el, props);
    for (let c of children) append(el, c);
    return el;
  }
  function clear(el) {
    for (; el.firstChild; ) el.removeChild(el.firstChild);
  }
  function kbd(combo) {
    let parts = combo ? combo.split("+") : [];
    return h("span", { class: "kbds" }, parts.map((p2) => h("kbd", null, p2)));
  }
  function cx(...names) {
    return names.filter(Boolean).join(" ");
  }

  // client/src/lib/keys.ts
  var DEFAULT_BINDINGS = [
    { command: "palette", label: "Command palette", group: "General", combo: "Ctrl+K" },
    { command: "view.changes", label: "Show Changes", group: "View", combo: "Ctrl+1" },
    { command: "view.history", label: "Show History", group: "View", combo: "Ctrl+2" },
    { command: "view.requests", label: "Show Change requests", group: "View", combo: "Ctrl+3" },
    { command: "view.documents", label: "Show Documents", group: "View", combo: "Ctrl+4" },
    { command: "view.agent", label: "Show Agent", group: "View", combo: "Ctrl+5" },
    { command: "view.activity", label: "Show Activity", group: "View", combo: "Ctrl+6" },
    { command: "repo.list", label: "Show project list", group: "View", combo: "Ctrl+T" },
    { command: "branch.list", label: "Show branch list", group: "View", combo: "Ctrl+B" },
    { command: "sidebar.toggle", label: "Toggle sidebar", group: "View", combo: "Ctrl+\\" },
    { command: "settings", label: "Settings", group: "General", combo: "Ctrl+," },
    { command: "theme.toggle", label: "Toggle light and dark", group: "General", combo: "Ctrl+Shift+L" },
    { command: "sync", label: "Push or sync", group: "Project", combo: "Ctrl+Shift+P" },
    { command: "fetch", label: "Fetch", group: "Project", combo: "Ctrl+Shift+T" },
    { command: "commit", label: "Commit staged changes", group: "Project", combo: "Ctrl+Enter" },
    { command: "stage.all", label: "Stage all changes", group: "Project", combo: "Ctrl+Shift+A" },
    { command: "branch.new", label: "New draft", group: "Branch", combo: "Ctrl+Shift+N" },
    { command: "branch.delete", label: "Withdraw draft", group: "Branch", combo: "Ctrl+Shift+D" },
    { command: "branch.discard", label: "Discard all changes", group: "Branch", combo: "Ctrl+Shift+Backspace" },
    { command: "branch.update", label: "Update from main", group: "Branch", combo: "Ctrl+Shift+U" },
    { command: "branch.compare", label: "Compare with main", group: "Branch", combo: "Ctrl+Shift+B" },
    { command: "branch.merge", label: "Merge into main", group: "Branch", combo: "Ctrl+Shift+M" },
    { command: "branch.rebase", label: "Rebase onto main", group: "Branch", combo: "Ctrl+Shift+E" },
    { command: "branch.request", label: "Send for review", group: "Branch", combo: "Ctrl+Alt+R" },
    { command: "agent.new", label: "New agent thread", group: "Agent", combo: "Ctrl+Shift+O" }
  ], MODS = ["Ctrl", "Alt", "Shift", "Meta"];
  function keyName(key) {
    return key === " " ? "Space" : key.length === 1 ? key.toUpperCase() : { Escape: "Esc", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right" }[key] ?? key;
  }
  function comboOf(e) {
    if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return "";
    let key = e.key;
    e.code && /^Digit[0-9]$/.test(e.code) && (key = e.code.slice(5)), e.code === "Comma" && (key = ","), e.code === "Backslash" && (key = "\\");
    let parts = [];
    return (e.ctrlKey || e.metaKey) && parts.push("Ctrl"), e.altKey && parts.push("Alt"), e.shiftKey && parts.push("Shift"), parts.push(keyName(key)), parts.join("+");
  }
  function normalize(combo) {
    let parts = combo.split("+").map((p2) => p2.trim()).filter(Boolean), mods = MODS.filter((m) => parts.some((p2) => p2.toLowerCase() === m.toLowerCase() || m === "Ctrl" && /^(control|cmd|command)$/i.test(p2))), rest = parts.filter((p2) => !/^(ctrl|control|cmd|command|alt|shift|meta)$/i.test(p2));
    return rest.length !== 1 ? "" : [...mods.filter((m) => m !== "Meta"), keyName((rest[0].length === 1, rest[0]))].join("+");
  }
  var RESERVED = ["Ctrl+R", "Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+A", "Ctrl+Z", "Ctrl+Y"];
  function conflicts(bindings) {
    let by = /* @__PURE__ */ new Map();
    for (let b of bindings) {
      let c = normalize(b.combo);
      c && by.set(c, [...by.get(c) ?? [], b.command]);
    }
    let out = [];
    for (let [combo, commands2] of by)
      commands2.length > 1 ? out.push({ combo, commands: commands2 }) : RESERVED.includes(combo) && out.push({ combo, commands: [...commands2, "(reserved)"] });
    return out;
  }
  function usable(combo) {
    let n = normalize(combo);
    return !n || RESERVED.includes(n) ? !1 : n.includes("+") || /^F([1-9]|1[0-2])$/.test(n);
  }
  function commandFor(bindings, e) {
    let c = comboOf(e);
    if (!c) return null;
    let hit = bindings.find((b) => normalize(b.combo) === c);
    return hit ? hit.command : null;
  }

  // client/src/lib/agentTools.ts
  var ACCESS_ORDER = ["read", "propose"], ACCESS_INFO = {
    read: { label: "Read only", blurb: "Reads history, diffs, documents and change requests. Changes nothing and proposes nothing." },
    propose: { label: "Propose", blurb: "Also drafts review comments and commit summaries as cards. A person posts or uses them." }
  }, TOOLS = [
    { name: "history.read", access: "read", summary: "Revisions of a file or project: author, time, chain hash", endpoint: "GET /documents/{id}/events" },
    { name: "history.verify", access: "read", summary: "Recomputes the hash chain of a file", endpoint: "GET /documents/{id}/verify" },
    { name: "diff.get", access: "read", summary: "What changed between two revisions, a draft and main, or two versions", endpoint: "GET /projects/{id}/agent/files/{file}/diff" },
    { name: "documents.list", access: "read", summary: "Files in the project with their latest revision", endpoint: "GET /projects/{id}/files" },
    { name: "documents.outline", access: "read", summary: "Headings, sheets or slides of one file", endpoint: "GET /projects/{id}/agent/files/{file}/outline" },
    { name: "requests.list", access: "read", summary: "Change requests and their states", endpoint: "GET /projects/{id}/change-requests" },
    { name: "requests.summarise", access: "read", summary: "A summary of one change request: what it changes and its review state", endpoint: "GET /documents/{id}/change-requests/{draft}" },
    { name: "review.propose_comment", access: "propose", summary: "Drafts a review comment on one change; a person posts it", endpoint: "none: returns a card" },
    { name: "commit.draft_summary", access: "propose", summary: "Drafts a commit summary and description from the staged changes; a person uses it", endpoint: "none: returns a card" }
  ], NEVER = [
    "Approve, request changes, merge or decline a change request. A person presses those buttons.",
    "Commit, push, withdraw a draft or rewrite history.",
    "Change roles, review rules or who may merge.",
    "Read a key, a token or a connection secret."
  ];
  function accessRank(a) {
    return ACCESS_ORDER.indexOf(a);
  }
  function mayCall(tool, access, enabled) {
    let t = TOOLS.find((x) => x.name === tool);
    return t ? enabled.includes(tool) ? accessRank(access) < accessRank(t.access) ? { ok: !1, reason: `it needs ${ACCESS_INFO[t.access].label} access and this thread has ${ACCESS_INFO[access].label}` } : { ok: !0 } : { ok: !1, reason: "it is switched off in Settings, Agent" } : { ok: !1, reason: "there is no such tool" };
  }
  var DEFAULT_INSTRUCTIONS = [
    "You are helping a team keep its documents under version control in Dynodoc.",
    "- Quote revisions by number (r12) and chain hash prefix.",
    "- Never approve or merge; say what a person should check before they do.",
    "- Keep review comments short, specific and kind.",
    "- Report what you read and what you did not."
  ].join(`
`);

  // client/src/lib/diff.ts
  function tokenize(text) {
    let out = [], re = /(\s+|[A-Za-z0-9À-ɏঀ-৿'’]+|[^\sA-Za-z0-9À-ɏঀ-৿'’])/gu, m;
    for (; (m = re.exec(text)) !== null; ) out.push(m[0]);
    return out;
  }
  function pushSeg(out, t, s2) {
    if (!s2) return;
    let last = out[out.length - 1];
    last && last.t === t ? last.s += s2 : out.push({ t, s: s2 });
  }
  function wordDiff(a, b) {
    if (a === b) return a ? [{ t: "same", s: a }] : [];
    let x = tokenize(a), y = tokenize(b), out = [];
    if (x.length * y.length > 25e4)
      return pushSeg(out, "del", a), pushSeg(out, "add", b), out;
    let n = x.length, m = y.length, L = new Uint32Array((n + 1) * (m + 1)), at = (i2, j2) => i2 * (m + 1) + j2;
    for (let i2 = n - 1; i2 >= 0; i2--)
      for (let j2 = m - 1; j2 >= 0; j2--)
        L[at(i2, j2)] = x[i2] === y[j2] ? L[at(i2 + 1, j2 + 1)] + 1 : Math.max(L[at(i2 + 1, j2)], L[at(i2, j2 + 1)]);
    let i = 0, j = 0, del = "", add = "", flush = () => {
      pushSeg(out, "del", del), pushSeg(out, "add", add), del = "", add = "";
    };
    for (; i < n && j < m; )
      x[i] === y[j] ? (flush(), pushSeg(out, "same", x[i]), i++, j++) : L[at(i + 1, j)] >= L[at(i, j + 1)] ? del += x[i++] : add += y[j++];
    for (; i < n; ) del += x[i++];
    for (; j < m; ) add += y[j++];
    return flush(), out;
  }
  function applyEdits(base, edits2) {
    let byId = new Map(edits2.filter((e) => e.op !== "add").map((e) => [e.blockId, e])), adds = edits2.filter((e) => e.op === "add"), known = /* @__PURE__ */ new Set([...base.map((b) => b.id), ...adds.map((e) => e.blockId)]), out = [], placed = /* @__PURE__ */ new Set(), addAfter = (anchor) => {
      for (let e of adds)
        (e.afterId ?? "") !== anchor || placed.has(e.blockId) || (placed.add(e.blockId), out.push({ id: e.blockId, label: e.label ?? e.blockId, text: e.after ?? "" }), addAfter(e.blockId));
    };
    addAfter("");
    for (let b of base) {
      let e = byId.get(b.id);
      e ? e.op === "edit" && out.push({ ...b, text: e.after ?? "" }) : out.push(b), addAfter(b.id);
    }
    for (let e of adds) e.afterId && !known.has(e.afterId) && !placed.has(e.blockId) && out.push({ id: e.blockId, label: e.label ?? e.blockId, text: e.after ?? "" });
    return out;
  }
  function rows(base, edits2) {
    let byId = new Map(edits2.filter((e) => e.op !== "add").map((e) => [e.blockId, e])), adds = edits2.filter((e) => e.op === "add"), known = /* @__PURE__ */ new Set([...base.map((b) => b.id), ...adds.map((e) => e.blockId)]), out = [], oldNo = 0, newNo = 0, placed = /* @__PURE__ */ new Set(), addAfter = (anchor) => {
      for (let e of adds)
        (e.afterId ?? "") !== anchor || placed.has(e.blockId) || (placed.add(e.blockId), out.push({ kind: "add", blockId: e.blockId, label: e.label ?? e.blockId, newNo: ++newNo, after: e.after ?? "" }), addAfter(e.blockId));
    };
    addAfter("");
    for (let b of base) {
      let e = byId.get(b.id);
      if (oldNo++, !e) out.push({ kind: "context", blockId: b.id, label: b.label, oldNo, newNo: ++newNo, before: b.text, after: b.text });
      else if (e.op === "remove") out.push({ kind: "del", blockId: b.id, label: b.label, oldNo, before: b.text });
      else {
        let after = e.after ?? "";
        out.push({ kind: "edit", blockId: b.id, label: b.label, oldNo, newNo: ++newNo, before: b.text, after, segs: wordDiff(b.text, after) });
      }
      addAfter(b.id);
    }
    for (let e of adds)
      e.afterId && !known.has(e.afterId) && !placed.has(e.blockId) && out.push({ kind: "add", blockId: e.blockId, label: e.label ?? e.blockId, newNo: ++newNo, after: e.after ?? "" });
    return out;
  }
  function hunks(base, edits2, context = 3) {
    let all = rows(base, edits2), ctx = Math.max(0, context), keep = new Array(all.length).fill(!1);
    all.forEach((r, i) => {
      if (r.kind === "context") return;
      let lo = Math.max(0, i - ctx), hi = Number.isFinite(ctx) ? Math.min(all.length - 1, i + ctx) : all.length - 1;
      for (let k = Number.isFinite(ctx) ? lo : 0; k <= hi; k++) keep[k] = !0;
    });
    let out = [], current3 = [], close = () => {
      if (!current3.length) return;
      let first = current3[0].label, last = current3[current3.length - 1].label;
      out.push({ header: first === last ? `@@ ${first} @@` : `@@ ${first} – ${last} @@`, rows: current3 }), current3 = [];
    };
    return all.forEach((r, i) => {
      keep[i] ? current3.push(r) : close();
    }), close(), out;
  }
  function unified(hunk) {
    let out = [];
    for (let r of hunk.rows)
      if (r.kind === "context") out.push({ sign: " ", label: r.label, segs: [{ t: "same", s: r.before ?? "" }], oldNo: r.oldNo, newNo: r.newNo, blockId: r.blockId });
      else if (r.kind === "del") out.push({ sign: "-", label: r.label, segs: [{ t: "del", s: r.before ?? "" }], oldNo: r.oldNo, blockId: r.blockId });
      else if (r.kind === "add") out.push({ sign: "+", label: r.label, segs: [{ t: "add", s: r.after ?? "" }], newNo: r.newNo, blockId: r.blockId });
      else {
        let segs2 = r.segs ?? [];
        out.push({ sign: "-", label: r.label, segs: segs2.filter((g) => g.t !== "add"), oldNo: r.oldNo, blockId: r.blockId }), out.push({ sign: "+", label: r.label, segs: segs2.filter((g) => g.t !== "del"), newNo: r.newNo, blockId: r.blockId });
      }
    return out;
  }
  function split(hunk) {
    return hunk.rows.map((r) => {
      let segs2 = r.segs ?? [], left = r.oldNo === void 0 ? void 0 : { label: r.label, no: r.oldNo, segs: r.kind === "edit" ? segs2.filter((g) => g.t !== "add") : [{ t: r.kind === "del" ? "del" : "same", s: r.before ?? "" }] }, right = r.newNo === void 0 ? void 0 : { label: r.label, no: r.newNo, segs: r.kind === "edit" ? segs2.filter((g) => g.t !== "del") : [{ t: r.kind === "add" ? "add" : "same", s: r.after ?? "" }] };
      return { kind: r.kind, blockId: r.blockId, left, right };
    });
  }
  function words(text) {
    return tokenize(text).filter((t) => /\S/.test(t) && /[\p{L}\p{N}]/u.test(t)).length;
  }
  function stats(edits2) {
    let out = { edited: 0, added: 0, removed: 0, wordsAdded: 0, wordsRemoved: 0 };
    for (let e of edits2)
      if (e.op === "add")
        out.added++, out.wordsAdded += words(e.after ?? "");
      else if (e.op === "remove")
        out.removed++, out.wordsRemoved += words(e.before ?? "");
      else {
        out.edited++;
        for (let g of wordDiff(e.before ?? "", e.after ?? ""))
          g.t === "add" && (out.wordsAdded += words(g.s)), g.t === "del" && (out.wordsRemoved += words(g.s));
      }
    return out;
  }
  function compareBlocks(from, to) {
    let toById = new Map(to.map((b) => [b.id, b])), fromIds = new Set(from.map((b) => b.id)), edits2 = [], before = "";
    for (let b of from) {
      let t = toById.get(b.id), anchor = before;
      before = b.id, t ? t.text !== b.text && edits2.push({ blockId: b.id, op: "edit", before: b.text, after: t.text }) : edits2.push({ blockId: b.id, op: "remove", before: b.text, afterId: anchor, label: b.label });
    }
    let previous = "";
    for (let t of to)
      fromIds.has(t.id) || edits2.push({ blockId: t.id, op: "add", after: t.text, afterId: previous, label: t.label }), previous = t.id;
    return edits2;
  }

  // client/src/lib/sha256.ts
  var K = new Uint32Array([
    1116352408,
    1899447441,
    3049323471,
    3921009573,
    961987163,
    1508970993,
    2453635748,
    2870763221,
    3624381080,
    310598401,
    607225278,
    1426881987,
    1925078388,
    2162078206,
    2614888103,
    3248222580,
    3835390401,
    4022224774,
    264347078,
    604807628,
    770255983,
    1249150122,
    1555081692,
    1996064986,
    2554220882,
    2821834349,
    2952996808,
    3210313671,
    3336571891,
    3584528711,
    113926993,
    338241895,
    666307205,
    773529912,
    1294757372,
    1396182291,
    1695183700,
    1986661051,
    2177026350,
    2456956037,
    2730485921,
    2820302411,
    3259730800,
    3345764771,
    3516065817,
    3600352804,
    4094571909,
    275423344,
    430227734,
    506948616,
    659060556,
    883997877,
    958139571,
    1322822218,
    1537002063,
    1747873779,
    1955562222,
    2024104815,
    2227730452,
    2361852424,
    2428436474,
    2756734187,
    3204031479,
    3329325298
  ]);
  function utf8(text) {
    return new TextEncoder().encode(text);
  }
  function sha256(input2) {
    let bytes = typeof input2 == "string" ? utf8(input2) : input2, bitLength = bytes.length * 8, padded = new Uint8Array(bytes.length + 9 + 63 >> 6 << 6);
    padded.set(bytes), padded[bytes.length] = 128;
    let view = new DataView(padded.buffer);
    view.setUint32(padded.length - 8, Math.floor(bitLength / 4294967296)), view.setUint32(padded.length - 4, bitLength >>> 0);
    let H = new Uint32Array([1779033703, 3144134277, 1013904242, 2773480762, 1359893119, 2600822924, 528734635, 1541459225]), W = new Uint32Array(64);
    for (let off = 0; off < padded.length; off += 64) {
      for (let i = 0; i < 16; i++) W[i] = view.getUint32(off + i * 4);
      for (let i = 16; i < 64; i++) {
        let a2 = W[i - 15], b2 = W[i - 2], s0 = (a2 >>> 7 | a2 << 25) ^ (a2 >>> 18 | a2 << 14) ^ a2 >>> 3, s1 = (b2 >>> 17 | b2 << 15) ^ (b2 >>> 19 | b2 << 13) ^ b2 >>> 10;
        W[i] = W[i - 16] + s0 + W[i - 7] + s1 >>> 0;
      }
      let a = H[0], b = H[1], c = H[2], d = H[3], e = H[4], f = H[5], g = H[6], h2 = H[7];
      for (let i = 0; i < 64; i++) {
        let S1 = (e >>> 6 | e << 26) ^ (e >>> 11 | e << 21) ^ (e >>> 25 | e << 7), ch = e & f ^ ~e & g, t1 = h2 + S1 + ch + K[i] + W[i] >>> 0, S0 = (a >>> 2 | a << 30) ^ (a >>> 13 | a << 19) ^ (a >>> 22 | a << 10), maj = a & b ^ a & c ^ b & c, t2 = S0 + maj >>> 0;
        h2 = g, g = f, f = e, e = d + t1 >>> 0, d = c, c = b, b = a, a = t1 + t2 >>> 0;
      }
      H[0] = H[0] + a >>> 0, H[1] = H[1] + b >>> 0, H[2] = H[2] + c >>> 0, H[3] = H[3] + d >>> 0, H[4] = H[4] + e >>> 0, H[5] = H[5] + f >>> 0, H[6] = H[6] + g >>> 0, H[7] = H[7] + h2 >>> 0;
    }
    let out = "";
    for (let i = 0; i < 8; i++) out += H[i].toString(16).padStart(8, "0");
    return out;
  }
  function chainHash(previous, contentHash) {
    return sha256(previous + ":" + contentHash);
  }
  var GENESIS = "0".repeat(64);

  // client/src/data/backend.ts
  var counter = 0;
  function uid(prefix) {
    return counter += 1, `${prefix}_${Date.now().toString(36)}${counter.toString(36)}`;
  }
  function canonical(edits2) {
    return JSON.stringify(edits2.map((e) => [e.op, e.blockId, e.before ?? null, e.after ?? null, e.afterId ?? null, e.label ?? null]));
  }
  function doc(db, id) {
    let d = db.docs.find((x) => x.id === id);
    if (!d) throw new Error("No such file.");
    return d;
  }
  function project(db, id) {
    let p2 = db.projects.find((x) => x.id === id);
    if (!p2) throw new Error("No such project.");
    return p2;
  }
  function draft(db, id) {
    return db.drafts.find((x) => x.id === id);
  }
  function person(db, id) {
    return db.people.find((p2) => p2.id === id) ?? { id, name: "Unknown", email: "" };
  }
  function docsOf(db, projectId) {
    return db.docs.filter((d) => d.projectId === projectId);
  }
  function branchCommits(db, docId, branch) {
    return db.commits.filter((c) => c.docId === docId && c.branch === branch).sort((a, b) => a.seq - b.seq);
  }
  function mainSeq(db, docId) {
    let list2 = branchCommits(db, docId, "main");
    return list2.length ? list2[list2.length - 1].seq : 0;
  }
  function blocksAt(db, docId, seq) {
    let blocks = doc(db, docId).blocks, later = branchCommits(db, docId, "main").filter((c) => c.seq > seq).reverse();
    for (let c of later) blocks = applyEdits(blocks, invert(c.edits));
    return blocks;
  }
  function blocksOf(db, docId, branch) {
    if (branch === "main") return doc(db, docId).blocks;
    let d = draft(db, branch);
    return d ? d.blocks : doc(db, docId).blocks;
  }
  function behind(db, d) {
    return Math.max(0, mainSeq(db, d.docId) - d.baseSeq);
  }
  function aheadOf(db, projectId, branch) {
    let ids = new Set(docsOf(db, projectId).map((d) => d.id));
    return db.commits.filter((c) => ids.has(c.docId) && c.branch === branch && c.local).length;
  }
  function lastChain(db, docId, branch) {
    let list2 = branchCommits(db, docId, branch);
    if (list2.length) return list2[list2.length - 1].chainHash;
    if (branch === "main") return GENESIS;
    let d = draft(db, branch), base = d ? branchCommits(db, docId, "main").filter((c) => c.seq <= d.baseSeq) : [];
    return base.length ? base[base.length - 1].chainHash : GENESIS;
  }
  function addCommit(db, input2) {
    let branchList = branchCommits(db, input2.docId, input2.branch), d = input2.branch === "main" ? void 0 : draft(db, input2.branch), seq = branchList.length ? branchList[branchList.length - 1].seq + 1 : 1, contentHash = sha256(canonical(input2.edits)), prev = lastChain(db, input2.docId, input2.branch), commit = {
      id: uid("rev"),
      docId: input2.docId,
      branch: input2.branch,
      seq,
      summary: input2.summary,
      description: input2.description ?? "",
      authorId: input2.authorId,
      at: input2.at ?? Date.now(),
      edits: input2.edits,
      kind: input2.kind ?? "revision",
      mergedFrom: input2.mergedFrom,
      contentHash,
      prevChainHash: prev,
      chainHash: chainHash(prev, contentHash),
      local: input2.local ?? !1
    };
    if (db.commits.push(commit), input2.branch === "main") {
      let target = doc(db, input2.docId);
      target.blocks = applyEdits(target.blocks, input2.edits);
    } else d && (d.blocks = applyEdits(d.blocks, input2.edits));
    return commit;
  }
  function verify(db, docId) {
    let out = [], branches = new Set(db.commits.filter((c) => c.docId === docId).map((c) => c.branch));
    for (let branch of branches) {
      let prev = branch === "main" ? GENESIS : "";
      for (let c of branchCommits(db, docId, branch)) {
        prev || (prev = c.prevChainHash);
        let content = sha256(canonical(c.edits)), ok = content === c.contentHash && c.prevChainHash === prev && chainHash(prev, content) === c.chainHash;
        out.push({ commitId: c.id, ok, detail: ok ? "chain hash recomputed and equal" : content !== c.contentHash ? "content hash differs from the edits" : "chain hash does not follow from the previous one" }), prev = c.chainHash;
      }
    }
    return out;
  }
  function log(db, item) {
    db.activity.unshift({ id: uid("act"), at: item.at ?? Date.now(), ...item });
  }
  function working(db, projectId, branch) {
    let ids = new Set(docsOf(db, projectId).map((d) => d.id));
    return db.working.filter((w) => ids.has(w.docId) && w.branch === branch);
  }
  function setStaged(db, ids, staged) {
    for (let w of db.working) ids.includes(w.id) && (w.staged = staged);
  }
  function discard(db, ids) {
    let before = db.working.length;
    return db.working = db.working.filter((w) => !ids.includes(w.id)), before - db.working.length;
  }
  function commitStaged(db, projectId, branch, summary, description) {
    let staged = working(db, projectId, branch).filter((w) => w.staged);
    if (!staged.length) throw new Error("Nothing is staged.");
    if (!summary.trim()) throw new Error("A summary is required.");
    let byDoc = /* @__PURE__ */ new Map();
    for (let w of staged) byDoc.set(w.docId, [...byDoc.get(w.docId) ?? [], w]);
    let out = [];
    for (let [docId, list2] of byDoc) {
      let current3 = blocksOf(db, docId, branch), edits2 = list2.map((w) => {
        let b = current3.find((x) => x.id === w.edit.blockId);
        return w.edit.op === "add" ? w.edit : { ...w.edit, before: b ? b.text : w.edit.before };
      });
      out.push(addCommit(db, { docId, branch, edits: edits2, summary: summary.trim(), description: description.trim(), authorId: db.me, local: !0 }));
    }
    return discard(db, staged.map((w) => w.id)), project(db, projectId).updatedAt = Date.now(), log(db, { actorId: db.me, projectId, kind: "commit", summary: `committed "${summary.trim()}" to ${branchName(db, branch)}` }), out;
  }
  function branchName(db, branch) {
    return branch === "main" ? "main" : draft(db, branch)?.name ?? branch;
  }
  function push(db, projectId, branch) {
    let ids = new Set(docsOf(db, projectId).map((d) => d.id)), n = 0;
    for (let c of db.commits)
      ids.has(c.docId) && c.branch === branch && c.local && (c.local = !1, n++);
    return n;
  }
  function newDraft(db, docId, name2) {
    let clean = name2.trim().replace(/\s+/g, "-").toLowerCase();
    if (!/^[a-z0-9][a-z0-9._-]{0,62}$/.test(clean)) throw new Error("A draft name uses letters, digits, dots, hyphens and underscores.");
    let target = doc(db, docId);
    if (db.drafts.some((d2) => d2.docId === docId && d2.name === clean && (d2.state === "private" || d2.state === "in_review")))
      throw new Error("This file already has an open draft with that name.");
    let d = {
      id: uid("draft"),
      docId,
      projectId: target.projectId,
      name: clean,
      authorId: db.me,
      baseSeq: mainSeq(db, docId),
      createdAt: Date.now(),
      state: "private",
      blocks: target.blocks.map((b) => ({ ...b }))
    };
    return db.drafts.push(d), log(db, { actorId: db.me, projectId: target.projectId, docId, kind: "draft", summary: `started the draft ${clean} of ${target.title}` }), d;
  }
  function withdraw(db, draftId) {
    let d = draft(db, draftId);
    if (!d) throw new Error("No such draft.");
    if (d.authorId !== db.me) throw new Error("Only its author can withdraw a draft.");
    d.state = "withdrawn";
    for (let r of db.requests) r.draftId === draftId && r.status === "open" && (r.status = "withdrawn");
    db.working = db.working.filter((w) => w.branch !== draftId), log(db, { actorId: db.me, projectId: d.projectId, docId: d.docId, kind: "draft", summary: `withdrew the draft ${d.name} (kept in history)` });
  }
  function draftEdits(db, d) {
    let target = doc(db, d.docId), base = baseBlocks(db, d, target);
    return compareBlocks(base, d.blocks);
  }
  function baseBlocks(db, d, target = doc(db, d.docId)) {
    return blocksAt(db, target.id, d.baseSeq);
  }
  function invert(edits2) {
    return edits2.map((e) => e.op === "add" ? { blockId: e.blockId, op: "remove", before: e.after } : e.op === "remove" ? { blockId: e.blockId, op: "add", after: e.before, label: e.label ?? e.blockId, afterId: e.afterId } : { blockId: e.blockId, op: "edit", before: e.after, after: e.before }).reverse();
  }
  function conflictsOf(db, d) {
    let target = doc(db, d.docId), base = baseBlocks(db, d, target), mine = new Map(compareBlocks(base, d.blocks).map((e) => [e.blockId, e])), out = [];
    for (let e of compareBlocks(base, target.blocks)) {
      let m = mine.get(e.blockId);
      if (!m || e.op !== "edit" || m.op !== "edit" || m.after === e.after) continue;
      let label = base.find((b) => b.id === e.blockId)?.label ?? e.blockId;
      out.push({ blockId: e.blockId, label, ancestor: e.before ?? "", team: e.after ?? "", draft: m.after ?? "" });
    }
    return out;
  }
  function rebase(db, draftId, choices) {
    let d = draft(db, draftId);
    if (!d) throw new Error("No such draft.");
    let conflicts2 = conflictsOf(db, d);
    for (let c of conflicts2) if (!choices[c.blockId]) throw new Error(`Choose a version for ${c.label}.`);
    let target = doc(db, d.docId), base = baseBlocks(db, d, target), mineEdits = compareBlocks(base, d.blocks), blocks = target.blocks.map((b) => ({ ...b })), kept = [];
    for (let e of mineEdits) {
      if (choices[e.blockId] === "team") continue;
      let cur = blocks.find((b) => b.id === e.blockId);
      kept.push(e.op === "edit" ? { ...e, before: cur?.text ?? e.before } : e);
    }
    blocks = applyEdits(blocks, kept);
    let changes = compareBlocks(d.blocks, blocks), from = d.baseSeq;
    if (d.baseSeq = mainSeq(db, d.docId), !changes.length)
      return log(db, { actorId: db.me, projectId: d.projectId, docId: d.docId, kind: "draft", summary: `updated ${d.name} from main (r${from} to r${d.baseSeq})` }), null;
    let commit = addCommit(db, {
      docId: d.docId,
      branch: d.id,
      edits: changes,
      summary: `Rebase onto main r${d.baseSeq}`,
      description: conflicts2.length ? `Resolved ${conflicts2.length} conflict${conflicts2.length === 1 ? "" : "s"}.` : "",
      authorId: db.me,
      kind: "rebase",
      local: !1
    });
    for (let r of db.requests)
      if (r.draftId === d.id && r.status === "open") {
        r.conflicts = conflictsOf(db, d), r.edits = draftEdits(db, d), r.baseSeq = d.baseSeq;
        for (let rv of r.reviews) rv.stale = !0;
      }
    return commit;
  }
  function nextRequestNumber(db, projectId) {
    return db.requests.filter((r) => r.projectId === projectId).reduce((n, r) => Math.max(n, r.number), 0) + 1;
  }
  function sendForReview(db, draftId, name2, note) {
    let d = draft(db, draftId);
    if (!d) throw new Error("No such draft.");
    if (d.state !== "private") throw new Error("This draft is already in review or closed.");
    let edits2 = draftEdits(db, d);
    if (!edits2.length) throw new Error("The draft has no changes against main.");
    let r = {
      id: uid("cr"),
      number: nextRequestNumber(db, d.projectId),
      projectId: d.projectId,
      docId: d.docId,
      draftId,
      name: name2.trim() || d.name,
      note: note.trim(),
      authorId: db.me,
      status: "open",
      createdAt: Date.now(),
      baseSeq: d.baseSeq,
      edits: edits2,
      excluded: [],
      conflicts: conflictsOf(db, d),
      reviews: [],
      comments: []
    };
    return db.requests.push(r), d.state = "in_review", log(db, { actorId: db.me, projectId: d.projectId, docId: d.docId, kind: "request", summary: `opened change request #${r.number} ${r.name}` }), r;
  }
  var MERGERS = { editors: ["owner", "manager", "editor"], owners: ["owner", "manager"] }, APPROVERS = ["owner", "manager", "editor", "approver"];
  function reviewState(db, r) {
    let p2 = project(db, r.projectId), latest = /* @__PURE__ */ new Map();
    for (let rv of r.reviews) !rv.stale && rv.verdict !== "commented" && latest.set(rv.reviewerId, rv.verdict);
    let approvals = [...latest.values()].filter((v) => v === "approved").length, changesRequested = [...latest.values()].filter((v) => v === "changes_requested").length, required = p2.rules.requiredApprovals, unresolved = r.conflicts.filter((c) => !c.choice).length, satisfied = approvals >= required && changesRequested === 0, mayMerge = MERGERS[p2.rules.mergeRoles].includes(p2.role), canMerge = r.status === "open" && satisfied && mayMerge && unresolved === 0, canApprove = r.status === "open" && APPROVERS.includes(p2.role) && r.authorId !== db.me, why = r.status !== "open" ? `This request is ${r.status}.` : mayMerge ? changesRequested ? "A reviewer asked for changes." : approvals < required ? `${approvals} of ${required} required approval${required === 1 ? "" : "s"}.` : unresolved ? `Choose a version for ${unresolved} conflict${unresolved === 1 ? "" : "s"}.` : "Ready to merge." : `Only ${p2.rules.mergeRoles === "owners" ? "owners and managers" : "editors, managers and owners"} merge in this project.`;
    return { approvals, required, changesRequested, satisfied, canMerge, canApprove, why };
  }
  function review(db, requestId, verdict, note) {
    let r = db.requests.find((x) => x.id === requestId);
    if (!r) throw new Error("No such request.");
    if (verdict !== "commented" && !reviewState(db, r).canApprove) throw new Error("You cannot review your own request, or your role does not review.");
    r.reviews.push({ reviewerId: db.me, verdict, note: note.trim(), at: Date.now(), stale: !1 }), note.trim() && r.comments.push({ id: uid("cmt"), authorId: db.me, text: note.trim(), at: Date.now() });
    let word = verdict === "approved" ? "approved" : verdict === "changes_requested" ? "requested changes on" : "commented on";
    log(db, { actorId: db.me, projectId: r.projectId, docId: r.docId, kind: "review", summary: `${word} #${r.number} ${r.name}` });
  }
  function comment(db, requestId, text, blockId, proposedByAgent = !1) {
    let r = db.requests.find((x) => x.id === requestId);
    if (!r) throw new Error("No such request.");
    if (!text.trim()) throw new Error("Write a comment first.");
    r.comments.push({ id: uid("cmt"), authorId: db.me, text: text.trim(), at: Date.now(), blockId, proposedByAgent });
  }
  function merge(db, requestId) {
    let r = db.requests.find((x) => x.id === requestId);
    if (!r) throw new Error("No such request.");
    let st = reviewState(db, r);
    if (!st.canMerge) throw new Error(st.why);
    let target = doc(db, r.docId), choice = new Map(r.conflicts.map((c) => [c.blockId, c.choice])), edits2 = [];
    for (let e of r.edits) {
      if (r.excluded.includes(e.blockId) || choice.get(e.blockId) === "team") continue;
      let cur = target.blocks.find((b) => b.id === e.blockId);
      e.op === "edit" && !cur || edits2.push(e.op === "add" ? e : { ...e, before: cur?.text ?? e.before });
    }
    let commit = addCommit(db, {
      docId: r.docId,
      branch: "main",
      edits: edits2,
      summary: `Merge #${r.number}: ${r.name}`,
      description: r.note,
      authorId: db.me,
      kind: "merge",
      mergedFrom: r.draftId
    });
    r.status = "merged", r.mergedAt = Date.now();
    let d = draft(db, r.draftId);
    return d && (d.state = "merged"), log(db, { actorId: db.me, projectId: r.projectId, docId: r.docId, kind: "merge", summary: `merged #${r.number} ${r.name} into main (r${commit.seq})` }), commit;
  }
  function decline(db, requestId, note) {
    let r = db.requests.find((x) => x.id === requestId);
    if (!r) throw new Error("No such request.");
    if (r.status !== "open") throw new Error(`This request is ${r.status}.`);
    r.status = "declined", note.trim() && r.comments.push({ id: uid("cmt"), authorId: db.me, text: note.trim(), at: Date.now() });
    let d = draft(db, r.draftId);
    d && (d.state = "withdrawn"), log(db, { actorId: db.me, projectId: r.projectId, docId: r.docId, kind: "request", summary: `declined #${r.number} ${r.name}` });
  }
  function recoveryDraft(db, commitId, mode) {
    let c = db.commits.find((x) => x.id === commitId);
    if (!c) throw new Error("No such revision.");
    let target = doc(db, c.docId), name2 = `${mode === "revert" ? "revert" : "pick"}-${c.branch === "main" ? "r" : "d"}${c.seq}`, d = newDraft(db, target.id, name2 + (db.drafts.some((x) => x.name === name2) ? "-" + Date.now().toString(36).slice(-3) : "")), source = mode === "revert" ? invert(c.edits) : c.edits, edits2 = [];
    for (let e of source) {
      let cur = d.blocks.find((b) => b.id === e.blockId);
      e.op === "edit" && cur ? edits2.push({ ...e, before: cur.text }) : e.op === "remove" && cur ? edits2.push({ ...e, before: cur.text }) : e.op === "add" && !cur && edits2.push(e);
    }
    if (!edits2.length) throw new Error("Those changes are already undone or no longer apply.");
    return addCommit(db, {
      docId: target.id,
      branch: d.id,
      edits: edits2,
      summary: mode === "revert" ? `Revert "${c.summary}"` : `Cherry-pick "${c.summary}"`,
      description: `${mode === "revert" ? "Reverts" : "Applies"} ${c.branch === "main" ? "r" : "d"}${c.seq} (${c.chainHash.slice(0, 8)}).`,
      authorId: db.me,
      kind: mode
    }), d;
  }
  function restoreVersion(db, docId, versionId) {
    let v = doc(db, docId).versions.find((x) => x.id === versionId);
    if (!v) throw new Error("No such version.");
    let then = blocksAt(db, docId, v.throughSeq), d = newDraft(db, docId, `restore-${v.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 40) || "version"}`), edits2 = compareBlocks(d.blocks, then);
    if (!edits2.length) throw new Error("main already matches this version.");
    return addCommit(db, { docId, branch: d.id, edits: edits2, summary: `Restore "${v.name}" as new changes`, authorId: db.me, kind: "restore" }), d;
  }
  function saveVersion(db, docId, name2, note) {
    let target = doc(db, docId);
    if (!name2.trim()) throw new Error("Name the version.");
    target.versions.unshift({ id: uid("ver"), name: name2.trim(), note: note.trim(), throughSeq: mainSeq(db, docId), at: Date.now(), authorId: db.me }), log(db, { actorId: db.me, projectId: target.projectId, docId, kind: "version", summary: `saved the version "${name2.trim()}" of ${target.title}` });
  }

  // client/src/data/sample.ts
  var MIN = 6e4, HOUR = 60 * MIN, DAY = 24 * HOUR;
  function edits(blocks, changes) {
    return changes.map((c) => {
      if (c[0] === "edit") return { blockId: c[1], op: "edit", before: blocks.find((b) => b.id === c[1])?.text ?? "", after: c[2] };
      if (c[0] === "add") return { blockId: c[1], op: "add", label: c[2], afterId: c[3], after: c[4] };
      let i = blocks.findIndex((b) => b.id === c[1]);
      return { blockId: c[1], op: "remove", before: blocks[i]?.text ?? "", label: blocks[i]?.label, afterId: i > 0 ? blocks[i - 1].id : "" };
    });
  }
  function seed(now = Date.now()) {
    let db = {
      me: "u_jordan",
      people: [
        { id: "u_jordan", name: "Jordan Lee", email: "jordan.lee@example.org" },
        { id: "u_alex", name: "Alex Morgan", email: "alex.morgan@example.org" },
        { id: "u_priya", name: "Priya Nair", email: "priya.nair@example.org" },
        { id: "u_sam", name: "Sam Okafor", email: "sam.okafor@example.org" },
        { id: "u_lena", name: "Lena Fischer", email: "lena.fischer@example.org" },
        { id: "u_tomas", name: "Tomás Ruiz", email: "tomas.ruiz@example.org" },
        { id: "u_mei", name: "Mei Chen", email: "mei.chen@example.org" },
        { id: "u_agent", name: "Review assistant", email: "agent key of Alex Morgan", agent: !0 }
      ],
      spaces: [
        {
          id: "sp_org",
          name: "Sample organisation",
          kind: "organization",
          members: [
            { personId: "u_alex", role: "owner", via: "direct" },
            { personId: "u_jordan", role: "manager", via: "direct" }
          ]
        },
        {
          id: "sp_ops",
          name: "Operations team",
          kind: "team",
          parentId: "sp_org",
          members: [
            { personId: "u_priya", role: "editor", via: "direct" },
            { personId: "u_sam", role: "editor", via: "direct" },
            { personId: "u_lena", role: "approver", via: "direct" },
            { personId: "u_tomas", role: "editor", via: "direct" },
            { personId: "u_mei", role: "reviewer", via: "direct" }
          ]
        }
      ],
      projects: [],
      docs: [],
      commits: [],
      drafts: [],
      requests: [],
      working: [],
      notifications: [],
      activity: [],
      deviceKeys: [
        { id: "key_laptop", name: "Jordan's laptop", kind: "person", publicKey: "b3f1c27e9a40d85e6c1f07b2a9d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f", createdAt: now - 40 * DAY, revoked: !1, scopes: ["project:read", "history:read", "history:write", "history:review"] },
        { id: "key_agent", name: "Review assistant", kind: "agent", publicKey: "5e8a1d2c3b4f6a7e8d9c0b1a2f3e4d5c6b7a8f9e0d1c2b3a4f5e6d7c8b9a0f1e", createdAt: now - 12 * DAY, revoked: !1, scopes: ["content:read"] }
      ],
      lastFetched: now - 4 * MIN,
      incoming: 0
    }, members = (roles) => roles.map(([personId, role]) => ({ personId, role, via: "direct" })), policy = { enabled: !0, minRole: "editor", callsPerMinute: 30, openProposals: 3, proposalsPerDay: 10, submitRequiresPerson: !0 }, proj = (p2) => {
      let full = {
        ownerId: "u_alex",
        private: !0,
        starred: !1,
        stars: 0,
        watch: "requests",
        updatedAt: now,
        members: [],
        issues: [],
        labels: [],
        agentPolicy: { ...policy },
        space: "sp_ops",
        ...p2
      };
      return db.projects.push(full), full;
    }, handbookProject = proj({
      id: "p_handbook",
      name: "Team handbook",
      description: "The team's handbook, its yearly budget and the quarterly review deck.",
      role: "manager",
      starred: !0,
      stars: 4,
      rules: { protectMain: !1, requiredApprovals: 2, mergeRoles: "editors" },
      members: members([
        ["u_alex", "owner"],
        ["u_jordan", "manager"],
        ["u_priya", "editor"],
        ["u_sam", "editor"],
        ["u_tomas", "editor"],
        ["u_lena", "approver"],
        ["u_mei", "reviewer"]
      ]),
      labels: [
        { id: "l_policy", name: "policy", color: "6d28d9" },
        { id: "l_budget", name: "budget", color: "0f766e" },
        { id: "l_question", name: "question", color: "b45309" }
      ]
    });
    handbookProject.issues = [
      { number: 3, title: "Remote work section does not mention working from another country", body: "Several people asked whether they can work from abroad for a few weeks. The handbook is silent on it.", state: "open", authorId: "u_tomas", assigneeId: "u_jordan", labels: ["l_policy"], comments: [{ authorId: "u_alex", at: now - 3 * DAY, text: "Agreed. Up to four weeks a year seems to be the common answer." }], createdAt: now - 6 * DAY },
      { number: 4, title: "Training budget looks low for next year", body: "Two courses are already booked for Q1. The plan line is 9,000.", state: "open", authorId: "u_lena", labels: ["l_budget", "l_question"], comments: [], createdAt: now - 2 * DAY },
      { number: 2, title: "Expenses deadline differs between handbook and finance page", body: "Handbook said 60 days, finance said 30.", state: "closed", authorId: "u_priya", labels: ["l_policy"], comments: [{ authorId: "u_priya", at: now - 26 * DAY, text: "Fixed by #9." }], createdAt: now - 30 * DAY, closedAt: now - 26 * DAY }
    ];
    let field2 = proj({
      id: "p_field",
      name: "Field guide",
      description: "How the team runs site visits: safety, checklists and contacts.",
      role: "editor",
      stars: 1,
      rules: { protectMain: !0, requiredApprovals: 1, mergeRoles: "owners" },
      members: members([
        ["u_alex", "owner"],
        ["u_jordan", "editor"],
        ["u_tomas", "editor"],
        ["u_lena", "approver"]
      ]),
      updatedAt: now - 4 * DAY
    }), grant = proj({
      id: "p_grant",
      name: "Grant application",
      description: "Narrative and budget for the community skills grant.",
      role: "reviewer",
      ownerId: "u_lena",
      rules: { protectMain: !0, requiredApprovals: 1, mergeRoles: "owners" },
      members: members([
        ["u_lena", "owner"],
        ["u_jordan", "reviewer"],
        ["u_priya", "editor"]
      ]),
      space: "sp_org",
      updatedAt: now - 8 * DAY
    }), mkDoc = (id, projectId, title, filename, kind, createdAt) => {
      let d = { id, projectId, title, filename, kind, blocks: [], versions: [], connections: [], createdAt, discussions: [], copies: [], publicLinks: [] };
      return db.docs.push(d), d;
    }, rev2 = (d, ago, authorId, summary, changes, extra = {}) => {
      let branch = extra.branch ?? "main", blocks = branch === "main" ? d.blocks : db.drafts.find((x) => x.id === branch).blocks;
      return addCommit(db, { docId: d.id, branch, edits: edits(blocks, changes), summary, description: extra.description, authorId, at: now - ago, kind: extra.kind, mergedFrom: extra.mergedFrom });
    }, mkDraft = (id, d, name2, authorId, ago, state2, baseSeq = mainSeq(db, d.id)) => {
      let dr = { id, docId: d.id, projectId: d.projectId, name: name2, authorId, baseSeq, createdAt: now - ago, state: state2, blocks: d.blocks.map((b) => ({ ...b })) };
      return db.drafts.push(dr), dr;
    }, key = (host, createdAgo, lastUsedAgo, revoked = !1) => ({ id: `ck_${host}_${createdAgo}`, host, createdAt: now - createdAgo, expiresAt: now - createdAgo + 7 * DAY, revoked, lastUsed: lastUsedAgo === void 0 ? void 0 : now - lastUsedAgo }), hb = mkDoc("d_handbook", "p_handbook", "Team handbook", "handbook.docx", "document", now - 60 * DAY), P = (n, heading = !1) => `¶ ${n}${heading ? " · Heading" : ""}`;
    rev2(hb, 60 * DAY, "u_alex", "Create the team handbook", [
      ["add", "p1", P(1, !0), "", "Welcome to the team"],
      ["add", "p2", P(2), "p1", "This handbook describes how we work together: our hours, how we plan, how we share documents and how we look after each other."],
      ["add", "p3", P(3, !0), "p2", "Working hours"],
      ["add", "p4", P(4), "p3", "Core hours are 10:00 to 16:00."],
      ["add", "p5", P(5, !0), "p4", "Remote work"],
      ["add", "p6", P(6), "p5", "You may work remotely up to two days a week. Agree the days with your manager at the start of each month."],
      ["add", "p8", P(8, !0), "p6", "Leave"],
      ["add", "p9", P(9), "p8", "Everyone has 22 days of annual leave plus public holidays."],
      ["add", "p10", P(10), "p9", "Parental leave is 12 weeks at full pay."],
      ["add", "p11", P(11, !0), "p10", "Expenses"],
      ["add", "p12", P(12), "p11", "Submit expenses within 60 days with a receipt. Travel is booked through the shared travel inbox."]
    ]), rev2(hb, 52 * DAY, "u_priya", "Shorten core hours", [["edit", "p4", "Core hours are 10:00 to 15:00 in your local time zone. Outside core hours, plan your day as suits you and your team."]], {
      description: "Agreed in the September planning meeting."
    }), rev2(hb, 45 * DAY, "u_alex", "Allow three remote days a week", [["edit", "p6", "You may work remotely up to three days a week. Agree the days with your manager at the start of each month."]]), rev2(hb, 40 * DAY, "u_sam", "Add the monthly in-person day", [["add", "p7", P(7), "p6", "The team meets in person on the first Tuesday of every month."]]), rev2(hb, 33 * DAY, "u_lena", "Annual leave goes to 25 days", [["edit", "p9", "Everyone has 25 days of annual leave plus public holidays."]]);
    let d9 = mkDraft("dr_expenses", hb, "expenses-deadline", "u_priya", 29 * DAY, "merged");
    rev2(hb, 29 * DAY, "u_priya", "Expenses within 30 days", [["edit", "p12", "Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox."]], { branch: d9.id }), rev2(hb, 26 * DAY, "u_alex", "Merge #9: Expenses within 30 days", [["edit", "p12", "Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox."]], {
      kind: "merge",
      mergedFrom: d9.id,
      description: "Matches the finance page. Closes #2."
    }), rev2(hb, 19 * DAY, "u_alex", "Add the documents and review section", [
      ["add", "p13", P(13, !0), "p12", "Documents And Review"],
      ["add", "p14", P(14), "p13", "Shared documents live in Dynodoc. Edits to the handbook go through a change request and need two approvals."]
    ]);
    let d10 = mkDraft("dr_fourday", hb, "four-day-week", "u_sam", 16 * DAY, "withdrawn");
    rev2(hb, 16 * DAY, "u_sam", "Trial a four-day week", [["edit", "p4", "From January the team trials a four-day week: core hours are 09:00 to 16:00, Monday to Thursday."]], { branch: d10.id });
    let d14 = mkDraft("dr_parental", hb, "parental-leave", "u_priya", 9 * DAY, "in_review");
    rev2(hb, 9 * DAY, "u_priya", "Parental leave: 16 weeks for every parent", [["edit", "p10", "Parental leave is 16 weeks at full pay for every parent, whoever gave birth, and can be taken in up to three blocks."]], {
      branch: d14.id,
      description: "Brings us in line with the policy the board approved."
    }), rev2(hb, 6 * DAY, "u_tomas", "Say how to request leave", [["edit", "p9", "Everyone has 25 days of annual leave plus public holidays. Request leave at least two weeks ahead in the leave calendar."]]), rev2(hb, 2 * DAY, "u_priya", "Fix heading case", [["edit", "p13", "Documents and review"]]);
    let mine = mkDraft("dr_remote", hb, "remote-work-update", "u_jordan", 1 * DAY, "private");
    rev2(hb, 1 * DAY, "u_jordan", "Allow working from another country", [
      ["edit", "p6", "You may work remotely up to three days a week, and from another country for up to four weeks a year. Agree the days with your manager at the start of each month."]
    ], { branch: mine.id, description: "Answers issue #3." }), hb.versions = [
      { id: "v_hb2", name: "Autumn 2026 edition", note: "Shared with new starters from October.", throughSeq: 7, at: now - 19 * DAY, authorId: "u_alex" },
      { id: "v_hb1", name: "First edition", note: "", throughSeq: 1, at: now - 60 * DAY, authorId: "u_alex" }
    ], hb.connections = [key("word", 2 * DAY, 1 * HOUR), key("google-docs", 20 * DAY, 19 * DAY)], hb.discussions = [
      { id: "th1", blockId: "p7", authorId: "u_tomas", text: "Does the first Tuesday still work for the people in other time zones?", at: now - 5 * DAY, resolved: !1, replies: [{ authorId: "u_alex", text: "We will check in the next planning meeting.", at: now - 4 * DAY }] },
      { id: "th2", blockId: "p4", authorId: "u_mei", text: "Core hours read well now.", at: now - 50 * DAY, resolved: !0, replies: [] }
    ], hb.copies = [{ id: "cp1", title: "handbook (edited).docx", relation: "edited_copy", score: 0.91, status: "open" }], hb.publicLinks = [{ id: "pl1", createdAt: now - 19 * DAY, expiresAt: now + 11 * DAY, revoked: !1 }];
    let bu = mkDoc("d_budget", "p_handbook", "Budget 2027", "budget-2027.xlsx", "spreadsheet", now - 40 * DAY), C = (cell, what) => `Budget!${cell} · ${what}`;
    rev2(bu, 40 * DAY, "u_sam", "Create the 2027 budget", [
      ["add", "C4", C("C4", "Salaries, plan"), "", "412,000"],
      ["add", "D4", C("D4", "Salaries, forecast"), "C4", "409,500"],
      ["add", "C5", C("C5", "Contractors, plan"), "D4", "48,000"],
      ["add", "D5", C("D5", "Contractors, forecast"), "C5", "48,000"],
      ["add", "C6", C("C6", "Software, plan"), "D5", "16,800"],
      ["add", "D6", C("D6", "Software, forecast"), "C6", "18,600"],
      ["add", "C7", C("C7", "Travel, plan"), "D6", "24,000"],
      ["add", "D7", C("D7", "Travel, forecast"), "C7", "24,000"],
      ["add", "C8", C("C8", "Training, plan"), "D7", "9,000"],
      ["add", "D8", C("D8", "Training, forecast"), "C8", "9,000"],
      ["add", "C9", C("C9", "Office, plan"), "D8", "30,000"],
      ["add", "D9", C("D9", "Office, forecast"), "C9", "30,000"],
      ["add", "C10", C("C10", "Total, plan"), "D9", "539,800  =SUM(C4:C9)"],
      ["add", "D10", C("D10", "Total, forecast"), "C10", "539,100  =SUM(D4:D9)"]
    ]), rev2(bu, 30 * DAY, "u_lena", "Contractors forecast up after Q2", [
      ["edit", "D5", "52,500"],
      ["edit", "D10", "543,600  =SUM(D4:D9)"]
    ]);
    let d11 = mkDraft("dr_software", bu, "software-renewals", "u_mei", 24 * DAY, "merged");
    rev2(bu, 24 * DAY, "u_mei", "Plan software renewals at the new price", [["edit", "C6", "18,600"], ["edit", "C10", "541,600  =SUM(C4:C9)"]], { branch: d11.id }), rev2(bu, 21 * DAY, "u_alex", "Merge #11: Plan software renewals at the new price", [["edit", "C6", "18,600"], ["edit", "C10", "541,600  =SUM(C4:C9)"]], { kind: "merge", mergedFrom: d11.id }), rev2(bu, 14 * DAY, "u_sam", "Office forecast after the lease renewal", [["edit", "D9", "31,200"], ["edit", "D10", "544,800  =SUM(D4:D9)"]]);
    let d15 = mkDraft("dr_travel", bu, "travel-reforecast", "u_sam", 8 * DAY, "in_review");
    rev2(bu, 8 * DAY, "u_sam", "Travel forecast for the January offsite", [["edit", "D7", "26,800"], ["edit", "D10", "547,600  =SUM(D4:D9)"]], {
      branch: d15.id,
      description: "Adds the offsite: 14 people, two nights."
    }), rev2(bu, 5 * DAY, "u_mei", "Travel forecast down: fewer conference trips", [["edit", "D7", "21,400"], ["edit", "D10", "542,200  =SUM(D4:D9)"]]), bu.versions = [{ id: "v_bu1", name: "Approved plan", note: "As approved by the board.", throughSeq: 3, at: now - 21 * DAY, authorId: "u_alex" }], bu.connections = [key("excel", 3 * DAY, 20 * MIN), key("google-sheets", 9 * DAY, void 0, !0)];
    let dk = mkDoc("d_deck", "p_handbook", "Quarterly review", "quarterly-review.pptx", "presentation", now - 20 * DAY), S = (n, part) => `Slide ${n} · ${part}`;
    rev2(dk, 20 * DAY, "u_mei", "Create the Q3 review deck", [
      ["add", "s1t", S(1, "Title"), "", "Quarterly review: Q3 2026"],
      ["add", "s1s", S(1, "Subtitle"), "s1t", "Team update, September 2026"],
      ["add", "s2t", S(2, "Title"), "s1s", "Highlights"],
      ["add", "s2b", S(2, "Body"), "s2t", "Handbook moved to Dynodoc · Budget forecast close to plan · Two new team members"],
      ["add", "s3t", S(3, "Title"), "s2b", "Budget"],
      ["add", "s3b", S(3, "Body"), "s3t", "Forecast 539,100 against a plan of 539,800."],
      ["add", "s5t", S(5, "Title"), "s3b", "Questions"]
    ]), rev2(dk, 9 * DAY, "u_mei", "Update the budget slide", [["edit", "s3b", "Forecast 544,800 against a plan of 541,600. Contractors and office above plan; travel on plan."]]), rev2(dk, 3 * DAY, "u_alex", "Add the next quarter slide", [
      ["add", "s4t", S(4, "Title"), "s3b", "Next quarter"],
      ["add", "s4b", S(4, "Body"), "s4t", "Finish the remote work policy · Review the training budget · Plan the January offsite"]
    ]);
    let d16 = mkDraft("dr_agentdate", dk, "fix-title-date", "u_agent", 5 * HOUR, "in_review");
    rev2(dk, 5 * HOUR, "u_agent", "Fix the month on the title slide", [["edit", "s1s", "Team update, October 2026"]], { branch: d16.id }), dk.connections = [key("powerpoint", 6 * DAY, 3 * DAY)];
    let fg = mkDoc("d_field", "p_field", "Field guide", "field-guide.docx", "document", now - 30 * DAY);
    rev2(fg, 30 * DAY, "u_alex", "Create the field guide", [
      ["add", "f1", P(1, !0), "", "Before a site visit"],
      ["add", "f2", P(2), "f1", "Tell the site contact the day before. Check the weather and the access notes."],
      ["add", "f3", P(3, !0), "f2", "On site"],
      ["add", "f4", P(4), "f3", "Wear the safety kit. Visits are done in pairs."],
      ["add", "f5", P(5, !0), "f4", "After the visit"],
      ["add", "f6", P(6), "f5", "Upload notes and photos within two working days."]
    ]), rev2(fg, 12 * DAY, "u_tomas", "Add the check-in call", [["edit", "f4", "Wear the safety kit. Visits are done in pairs, with a check-in call to the office every three hours."]]), fg.connections = [key("google-docs", 4 * DAY, 4 * DAY)];
    let gn = mkDoc("d_grant", "p_grant", "Grant narrative", "grant-narrative.docx", "document", now - 15 * DAY);
    rev2(gn, 15 * DAY, "u_lena", "First draft of the narrative", [
      ["add", "g1", P(1, !0), "", "Community skills programme"],
      ["add", "g2", P(2), "g1", "We will run twelve evening workshops on spreadsheets and budgeting for local volunteers."],
      ["add", "g3", P(3), "g2", "Each workshop takes up to 20 people. Materials are published under an open licence."]
    ]);
    let gb = mkDoc("d_grantbudget", "p_grant", "Grant budget", "grant-budget.xlsx", "spreadsheet", now - 15 * DAY);
    rev2(gb, 15 * DAY, "u_lena", "Grant budget lines", [
      ["add", "B3", "Lines!B3 · Trainers", "", "7,200"],
      ["add", "B4", "Lines!B4 · Venue", "B3", "2,400"],
      ["add", "B5", "Lines!B5 · Materials", "B4", "900"],
      ["add", "B6", "Lines!B6 · Total", "B5", "10,500  =SUM(B3:B5)"]
    ]), rev2(gb, 8 * DAY, "u_priya", "Venue at the community centre rate", [["edit", "B4", "1,800"], ["edit", "B6", "9,900  =SUM(B3:B5)"]]);
    let cr = (o) => {
      let d = db.drafts.find((x) => x.id === o.draftId), r = {
        projectId: "p_handbook",
        note: "",
        baseSeq: d.baseSeq,
        edits: draftEdits(db, d),
        excluded: [],
        conflicts: o.status === "open" ? conflictsOf(db, d) : [],
        reviews: [],
        comments: [],
        ...o
      };
      return db.requests.push(r), r;
    };
    cr({ id: "cr9", number: 9, docId: hb.id, draftId: d9.id, name: "Expenses within 30 days", authorId: "u_priya", status: "merged", createdAt: now - 29 * DAY, mergedAt: now - 26 * DAY, note: "Matches the finance page.", reviews: [
      { reviewerId: "u_alex", verdict: "approved", note: "", at: now - 27 * DAY, stale: !1 },
      { reviewerId: "u_lena", verdict: "approved", note: "Thanks.", at: now - 26 * DAY, stale: !1 }
    ] }), cr({ id: "cr10", number: 10, docId: hb.id, draftId: d10.id, name: "Trial a four-day week", authorId: "u_sam", status: "declined", createdAt: now - 16 * DAY, note: "For discussion.", comments: [{ id: "c10a", authorId: "u_alex", text: "Not this year; let us revisit in the spring.", at: now - 15 * DAY }] }), cr({ id: "cr11", number: 11, docId: bu.id, draftId: d11.id, name: "Plan software renewals at the new price", authorId: "u_mei", status: "merged", createdAt: now - 24 * DAY, mergedAt: now - 21 * DAY, reviews: [
      { reviewerId: "u_sam", verdict: "approved", note: "", at: now - 22 * DAY, stale: !1 },
      { reviewerId: "u_alex", verdict: "approved", note: "", at: now - 21 * DAY, stale: !1 }
    ] }), cr({
      id: "cr14",
      number: 14,
      docId: hb.id,
      draftId: d14.id,
      name: "Parental leave: 16 weeks for every parent",
      authorId: "u_priya",
      status: "open",
      createdAt: now - 9 * DAY,
      note: "Brings the handbook in line with the policy the board approved on 2 October.",
      reviews: [
        { reviewerId: "u_lena", verdict: "commented", note: "Can we say who to tell, and how early?", at: now - 3 * DAY, stale: !1 },
        { reviewerId: "u_alex", verdict: "approved", note: "Looks right to me.", at: now - 1 * DAY, stale: !1 }
      ],
      comments: [
        { id: "c14a", authorId: "u_lena", blockId: "p10", text: "Can we say who to tell, and how early?", at: now - 3 * DAY },
        { id: "c14b", authorId: "u_priya", blockId: "p10", text: "Good point: I will add it in a separate change so this one stays small.", at: now - 2 * DAY }
      ]
    }), cr({ id: "cr15", number: 15, docId: bu.id, draftId: d15.id, name: "Travel forecast for the January offsite", authorId: "u_sam", status: "open", createdAt: now - 3 * HOUR, note: "Adds the offsite: 14 people, two nights." }), cr({
      id: "cr16",
      number: 16,
      docId: dk.id,
      draftId: d16.id,
      name: "Fix the month on the title slide",
      authorId: "u_agent",
      status: "open",
      createdAt: now - 5 * HOUR,
      byAgent: !0,
      agentStatement: "The subtitle says September, but the deck covers the quarter that ended on 30 September and is presented in October. I changed the month to October. I did not check the figures on the other slides."
    }), db.working = [
      { id: "w1", docId: hb.id, branch: "main", staged: !0, source: "word", capturedAt: now - 40 * MIN, edit: { blockId: "p12", op: "edit", before: hb.blocks.find((b) => b.id === "p12").text, after: "Submit expenses within 30 days with a receipt. Travel is booked through the shared travel inbox at least ten days ahead." } },
      { id: "w2", docId: hb.id, branch: "main", staged: !1, source: "word", capturedAt: now - 35 * MIN, edit: { blockId: "p2", op: "edit", before: hb.blocks.find((b) => b.id === "p2").text, after: "This handbook describes how we work together: our hours, how we plan, how we share documents, and how we look after one another." } },
      { id: "w3", docId: bu.id, branch: "main", staged: !0, source: "excel", capturedAt: now - 22 * MIN, edit: { blockId: "C8", op: "edit", before: "9,000", after: "12,000" } },
      { id: "w4", docId: bu.id, branch: "main", staged: !0, source: "excel", capturedAt: now - 22 * MIN, edit: { blockId: "C10", op: "edit", before: bu.blocks.find((b) => b.id === "C10").text, after: "544,600  =SUM(C4:C9)" } },
      { id: "w5", docId: dk.id, branch: "main", staged: !1, source: "powerpoint", capturedAt: now - 9 * MIN, edit: { blockId: "s4b", op: "edit", before: dk.blocks.find((b) => b.id === "s4b").text, after: "Finish the remote work policy · Review the training budget (+3,000 in the plan) · Plan the January offsite" } },
      { id: "w6", docId: dk.id, branch: "main", staged: !1, source: "powerpoint", capturedAt: now - 8 * MIN, edit: { blockId: "s4n", op: "add", afterId: "s4b", label: "Slide 4 · Notes", after: "Ask Sam for the offsite numbers before the meeting." } },
      { id: "w7", docId: hb.id, branch: mine.id, staged: !0, source: "word", capturedAt: now - 15 * MIN, edit: { blockId: "p7", op: "edit", before: hb.blocks.find((b) => b.id === "p7").text, after: "The team meets in person on the first Tuesday of every month. Travel to it is paid from the team budget." } }
    ], db.notifications = [
      { id: "n1", kind: "change_request_submitted", at: now - 3 * HOUR, read: !1, actorId: "u_sam", projectId: "p_handbook", docId: bu.id, requestId: "cr15", text: "opened #15 Travel forecast for the January offsite" },
      { id: "n2", kind: "change_request_submitted", at: now - 5 * HOUR, read: !1, actorId: "u_agent", projectId: "p_handbook", docId: dk.id, requestId: "cr16", text: "proposed #16 Fix the month on the title slide" },
      { id: "n3", kind: "change_request_reviewed", at: now - 1 * DAY, read: !1, actorId: "u_alex", projectId: "p_handbook", docId: hb.id, requestId: "cr14", text: "approved #14 Parental leave: 16 weeks for every parent" },
      { id: "n4", kind: "copy_detected", at: now - 2 * DAY, read: !0, actorId: "u_mei", projectId: "p_handbook", docId: hb.id, text: "uploaded handbook (edited).docx, an edited copy of Team handbook (91% similar)" },
      { id: "n5", kind: "access_granted", at: now - 8 * DAY, read: !0, actorId: "u_lena", projectId: "p_grant", text: "gave you Contributor access to Grant application" },
      { id: "n6", kind: "change_request_merged", at: now - 21 * DAY, read: !0, actorId: "u_alex", projectId: "p_handbook", docId: bu.id, requestId: "cr11", text: "merged #11 Plan software renewals at the new price" }
    ];
    let act2 = (ago, actorId, projectId, kind, summary, docId, byAgent = !1) => db.activity.push({ id: `a${db.activity.length}`, at: now - ago, actorId, projectId, kind, summary, docId, byAgent });
    return act2(5 * HOUR, "u_agent", "p_handbook", "request", "proposed #16 Fix the month on the title slide", dk.id, !0), act2(3 * HOUR, "u_sam", "p_handbook", "request", "opened #15 Travel forecast for the January offsite", bu.id), act2(1 * DAY, "u_jordan", "p_handbook", "draft", "pushed remote-work-update (1 commit)", hb.id), act2(1 * DAY, "u_alex", "p_handbook", "review", "approved #14 Parental leave: 16 weeks for every parent", hb.id), act2(2 * DAY, "u_priya", "p_handbook", "commit", 'committed "Fix heading case" to main (r9)', hb.id), act2(3 * DAY, "u_alex", "p_handbook", "commit", 'committed "Add the next quarter slide" to main (r3)', dk.id), act2(5 * DAY, "u_mei", "p_handbook", "commit", 'committed "Travel forecast down: fewer conference trips" to main (r5)', bu.id), act2(6 * DAY, "u_tomas", "p_handbook", "issue", "opened issue #3 Remote work section does not mention working from another country"), act2(6 * DAY, "u_tomas", "p_handbook", "commit", 'committed "Say how to request leave" to main', hb.id), act2(8 * DAY, "u_priya", "p_grant", "commit", 'committed "Venue at the community centre rate" to main', gb.id), act2(12 * DAY, "u_tomas", "p_field", "commit", 'committed "Add the check-in call" to main', fg.id), act2(19 * DAY, "u_alex", "p_handbook", "version", 'saved the version "Autumn 2026 edition" of Team handbook', hb.id), act2(21 * DAY, "u_alex", "p_handbook", "merge", "merged #11 Plan software renewals at the new price", bu.id), db.activity.sort((a, b) => b.at - a.at), db;
  }
  var INCOMING = [
    {
      summary: "Lena Fischer approved #14",
      apply: (db) => {
        let r = db.requests.find((x) => x.id === "cr14");
        return r && r.status === "open" && (r.reviews.push({ reviewerId: "u_lena", verdict: "approved", note: "Fine as it is; the notice period can follow.", at: Date.now(), stale: !1 }), db.notifications.unshift({ id: `n_in${Date.now()}`, kind: "change_request_reviewed", at: Date.now(), read: !1, actorId: "u_lena", projectId: "p_handbook", docId: "d_handbook", requestId: "cr14", text: "approved #14 Parental leave: 16 weeks for every parent" })), "Lena Fischer approved #14. It now has the two approvals it needs.";
      }
    },
    {
      summary: "Tomás Ruiz committed to main",
      apply: (db) => {
        let d = db.docs.find((x) => x.id === "d_handbook"), p2 = d.blocks.find((b) => b.id === "p14");
        return p2 && addCommit(db, {
          docId: d.id,
          branch: "main",
          edits: [{ blockId: "p14", op: "edit", before: p2.text, after: "Shared documents live in Dynodoc. Edits to the handbook go through a change request and need two approvals; anyone may comment." }],
          summary: "Say that anyone may comment",
          authorId: "u_tomas"
        }), 'Tomás Ruiz committed "Say that anyone may comment" to main. Your drafts of the handbook are one revision further behind.';
      }
    }
  ];

  // client/src/state.ts
  var DEFAULT_PREFS = {
    theme: "system",
    palette: "neutral",
    accent: "",
    density: "comfortable",
    fontSize: "default",
    dateFormat: "medium",
    timeFormat: "24h",
    numberFormat: "comma",
    absoluteDates: !1,
    diffLayout: "unified",
    diffContext: 2,
    wordDiff: !0,
    tabSize: 4,
    authorName: "Jordan Lee",
    authorEmail: "jordan.lee@example.org",
    signCommits: !0,
    blame: !0,
    editorFor: { document: "office", spreadsheet: "office", presentation: "office" },
    notify: { enabled: !0, requests: !0, reviews: !0, merges: !0, copies: !0, access: !0 },
    prompts: { discard: !0, withdraw: !0, merge: !0, decline: !0, recovery: !0, switchWithChanges: "ask", commitLength: !0 },
    backgroundFetch: !0,
    fetchMinutes: 10,
    usageStats: !1,
    statusIcons: !0,
    underlineLinks: !1,
    checkMarks: !0,
    reduceMotion: !1,
    bindings: DEFAULT_BINDINGS.map((b) => ({ ...b })),
    agentTools: TOOLS.map((t) => t.name),
    agentAccess: "read",
    instructions: DEFAULT_INSTRUCTIONS,
    ladder: [
      { id: "lp1", kind: "anthropic", enabled: !1, model: "claude-sonnet-4-5", base: "https://api.anthropic.com", keyHint: "" },
      { id: "lp2", kind: "openai", enabled: !1, model: "gpt-5", base: "https://api.openai.com/v1", keyHint: "" },
      { id: "lp3", kind: "ollama", enabled: !1, model: "llama3.1", base: "http://127.0.0.1:11434", keyHint: "" }
    ],
    skills: [
      { id: "sk_summary", name: "Summarise a change request", description: "What a request changes, who reviewed it and what is left before it can merge.", enabled: !0, tools: ["requests.list", "requests.summarise", "diff.get"], builtIn: !0, body: `Read the request and its diff.
List the changed blocks by label, then the review state.
End with what a person must still do (approvals, conflicts).` },
      { id: "sk_budget", name: "Check a budget change", description: "Looks for totals that no longer add up after a cell edit.", enabled: !0, tools: ["diff.get", "documents.outline"], builtIn: !0, body: `For each edited cell, find the total that sums it.
Say whether the total was edited by the same change.
Never change a figure yourself.` },
      { id: "sk_commit", name: "Draft a commit summary", description: "A summary of up to 60 characters and a short description from the staged changes.", enabled: !0, tools: ["commit.draft_summary"], builtIn: !0, body: `Summary: imperative, at most 60 characters.
Description: one line per file, what changed and why if the text says.` },
      { id: "sk_chain", name: "Check the hash chain", description: "Recomputes a file’s chain and reports the first revision that does not verify.", enabled: !0, tools: ["history.verify", "history.read"], builtIn: !0, body: `Verify the chain of the file asked about.
Quote the revision and the hash prefix of any failure.` },
      { id: "sk_tone", name: "Kind review comments", description: "Keeps proposed review comments short, specific and kind.", enabled: !1, tools: ["review.propose_comment"], builtIn: !1, body: `One point per comment.
Quote the words you mean.
Suggest, do not order.` }
    ],
    sidebarOpen: !0
  }, state = {
    db: seed(),
    ui: {
      view: "changes",
      projectId: "p_handbook",
      branchByProject: { p_handbook: "main", p_field: "main", p_grant: "main" },
      changes: { selected: null, filter: "", summary: "", description: "", amend: !1 },
      history: { selected: null, mode: "list", filter: "", compareBranch: "", docFilter: "" },
      requests: { filter: "open", selected: null, comment: "" },
      documents: { selected: null, tab: "overview" },
      projects: { selected: null, tab: "overview", filter: "", type: "all", issue: null },
      activity: { tab: "notifications" },
      settings: { section: "general", filter: "" },
      agent: { threads: [], active: null, draft: "", detailsOpen: !0 },
      connectors: { filter: "", category: "all" },
      verified: {},
      toasts: [],
      fetching: !1,
      online: !0
    },
    prefs: structuredCloneSafe(DEFAULT_PREFS),
    /** Provider keys typed in Settings: in memory only, never rendered, never sent. */
    keys: /* @__PURE__ */ new Map(),
    /** The desktop shell's bridge, when this page runs inside the app's window. */
    desktop: !1,
    /** The desktop app's version, from its get_state command. */
    desktopVersion: ""
  };
  function structuredCloneSafe(v) {
    return JSON.parse(JSON.stringify(v));
  }
  function branchOf(projectId = state.ui.projectId) {
    return state.ui.branchByProject[projectId] ?? "main";
  }
  var scheduled = !1, renderer = () => {
  };
  function setRenderer(fn) {
    renderer = fn;
  }
  function render() {
    scheduled || (scheduled = !0, requestAnimationFrame(() => {
      scheduled = !1, renderer();
    }));
  }
  var toastId = 0;
  function toast(kind, title, text) {
    let id = ++toastId;
    state.ui.toasts = [...state.ui.toasts.slice(-3), { id, kind, title, text }], render(), setTimeout(() => {
      state.ui.toasts = state.ui.toasts.filter((t) => t.id !== id), render();
    }, kind === "error" ? 7e3 : 4500);
  }
  function go(view) {
    state.ui.view = view, render();
  }
  function resetSample() {
    state.db = seed(), state.ui.verified = {}, state.ui.changes = { selected: null, filter: "", summary: "", description: "", amend: !1 }, state.ui.history.selected = null, state.ui.requests.selected = null, state.ui.branchByProject = { p_handbook: "main", p_field: "main", p_grant: "main" }, render();
  }

  // client/src/theme.ts
  function applyAppearance() {
    let p2 = state.prefs, root2 = document.documentElement;
    p2.theme === "system" ? root2.removeAttribute("data-scheme") : root2.setAttribute("data-scheme", p2.theme), root2.setAttribute("data-palette", p2.palette), root2.setAttribute("data-density", p2.density), root2.setAttribute("data-font", p2.fontSize), root2.toggleAttribute("data-reduce-motion", p2.reduceMotion), root2.toggleAttribute("data-underline", p2.underlineLinks), p2.accent ? root2.style.setProperty("--accent-custom", p2.accent) : root2.style.removeProperty("--accent-custom"), root2.toggleAttribute("data-accent", !!p2.accent);
  }
  function scheme() {
    let p2 = state.prefs.theme;
    return p2 !== "system" ? p2 : window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  function toggleTheme() {
    state.prefs.theme = scheme() === "dark" ? "light" : "dark", applyAppearance();
  }

  // client/src/lib/branchMenu.ts
  var WRITERS = /* @__PURE__ */ new Set(["owner", "manager", "editor"]), PROPOSERS = /* @__PURE__ */ new Set(["owner", "manager", "editor", "approver", "reviewer"]);
  function canPropose(role) {
    return PROPOSERS.has(role);
  }
  function branchMenu(c) {
    let draft2 = c.kind === "draft", live = draft2 && (c.state === "private" || c.state === "in_review"), mine = c.mine !== !1, propose = canPropose(c.role), a = (id, label, shortcut, supported, enabled, note, group = !1) => ({
      id,
      label,
      shortcut,
      supported,
      enabled: supported && enabled,
      note,
      group
    });
    return [
      a("new", "New draft…", "Ctrl+Shift+N", !0, propose, propose ? "A personal draft of one file, based on the latest main." : "Your role cannot propose changes."),
      a("rename", "Rename…", "Ctrl+Shift+R", !1, !1, "Dynodoc drafts keep the name they were made with."),
      a(
        "delete",
        "Withdraw draft…",
        "Ctrl+Shift+D",
        !0,
        live && mine,
        draft2 ? live ? mine ? "Closes the draft; it stays in history as withdrawn." : "Only its author can withdraw a draft." : "This draft is already closed." : "main cannot be withdrawn."
      ),
      a("discard", "Discard all changes…", "Ctrl+Shift+Backspace", !0, (c.uncommitted ?? 0) > 0, (c.uncommitted ?? 0) > 0 ? "Drops the captured changes on this computer." : "There are no captured changes.", !0),
      a("stash", "Stash all changes", "Ctrl+Shift+S", !1, !1, "Dynodoc has no stash: commit to a draft instead."),
      a(
        "update",
        "Update from main",
        "Ctrl+Shift+U",
        !0,
        live && mine && (c.behind ?? 0) > 0,
        draft2 ? (c.behind ?? 0) > 0 ? `Brings in ${c.behind} newer revision${c.behind === 1 ? "" : "s"} of main.` : "The draft has the latest main." : "You are on main.",
        !0
      ),
      a("compare", "Compare with main", "Ctrl+Shift+B", !0, draft2, draft2 ? "Shows what the draft changes against main." : "Choose a draft to compare."),
      a(
        "merge",
        "Merge into main…",
        "Ctrl+Shift+M",
        !0,
        draft2 && c.state === "in_review" && !!c.requestSatisfied,
        draft2 ? c.state !== "in_review" ? "Send the draft for review first." : c.requestSatisfied ? "Opens the review to merge; a person presses Merge." : "The review rules are not met yet." : "Choose a draft. Drafts reach main through a change request."
      ),
      a("squash", "Squash and merge…", "Ctrl+Shift+H", !1, !1, "Dynodoc merges the selected changes of a request; it has no squash."),
      a(
        "rebase",
        "Rebase onto main…",
        "Ctrl+Shift+E",
        !0,
        live && mine && (c.behind ?? 0) > 0,
        draft2 ? (c.behind ?? 0) > 0 ? "Replays the draft on the latest main; you choose for each conflict." : "Nothing to rebase: the draft has the latest main." : "You are on main."
      ),
      a(
        "request",
        "Send for review",
        "Ctrl+Alt+R",
        !0,
        draft2 && c.state === "private" && mine && (c.ahead ?? 0) === 0,
        draft2 ? c.state !== "private" ? "This draft is already in review or closed." : (c.ahead ?? 0) > 0 ? "Push the draft first." : "Opens a change request from this draft." : "Choose a draft to send for review.",
        !0
      ),
      a("view", "View change request", "Ctrl+Alt+P", !0, draft2 && c.state === "in_review", draft2 && c.state === "in_review" ? "Opens the review." : "No open change request.")
    ];
  }
  function commitTarget(c) {
    return c.kind === "draft" ? c.state !== "private" && c.state !== "in_review" ? { ok: !1, note: "This draft is closed." } : c.mine === !1 ? { ok: !1, note: "This draft belongs to someone else." } : { ok: canPropose(c.role), note: canPropose(c.role) ? "" : "Your role cannot propose changes." } : WRITERS.has(c.role) ? c.protectedMain ? { ok: !1, note: "main is protected in this project: commit to a draft and send it for review." } : { ok: !0, note: "" } : { ok: !1, note: "Your role cannot write to main. Commit to a draft and send it for review." };
  }

  // client/src/icons.ts
  var I = {
    changes: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5", "M9 14h6", "M12 11v6"],
    history: ["M3 12a9 9 0 1 0 3-6.7", "M3 4v4h4", "M12 7v5l3 2"],
    request: [{ c: [6, 6, 2.2] }, { c: [6, 18, 2.2] }, { c: [18, 18, 2.2] }, "M6 8.2v7.6", "M18 15.8V11a3 3 0 0 0-3-3h-4", "M13 5.5 10.5 8l2.5 2.5"],
    documents: ["M8 3h8l4 4v11a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z", "M16 3v4h4", "M3 7v12a2 2 0 0 0 2 2h9"],
    projects: ["M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"],
    connectors: ["M9 7V3", "M15 7V3", "M6 7h12v4a6 6 0 0 1-12 0z", "M12 17v4"],
    activity: ["M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9", "M10.3 21a1.9 1.9 0 0 0 3.4 0"],
    settings: [{ c: [12, 12, 3] }, "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"],
    agent: ["M12 3l1.8 4.2L18 9l-4.2 1.8L12 15l-1.8-4.2L6 9l4.2-1.8z", "M19 15l.8 1.7 1.7.8-1.7.8L19 20l-.8-1.7-1.7-.8 1.7-.8z", "M5 16l.6 1.3 1.4.7-1.4.6L5 20l-.6-1.4L3 18l1.4-.7z"],
    branch: [{ c: [6, 5, 2.2] }, { c: [6, 19, 2.2] }, { c: [18, 7, 2.2] }, "M6 7.2v9.6", "M18 9.2c0 4-6 3.5-11 7.8"],
    repo: ["M5 4.5A2.5 2.5 0 0 1 7.5 2H19v16H7.5A2.5 2.5 0 0 0 5 20.5z", "M5 20.5A2.5 2.5 0 0 0 7.5 23H19v-5", "M9 6h6"],
    sync: ["M20 11a8 8 0 0 0-14.8-4", "M4 4v4h4", "M4 13a8 8 0 0 0 14.8 4", "M20 20v-4h-4"],
    up: ["M12 19V5", "M6 11l6-6 6 6"],
    down: ["M12 5v14", "M6 13l6 6 6-6"],
    search: [{ c: [11, 11, 7] }, "M20 20l-3.5-3.5"],
    plus: ["M12 5v14", "M5 12h14"],
    check: ["M5 12.5l4.5 4.5L19 7.5"],
    x: ["M6 6l12 12", "M18 6L6 18"],
    chevron: ["M6 9l6 6 6-6"],
    chevronRight: ["M9 6l6 6-6 6"],
    more: [{ c: [5, 12, 1.2] }, { c: [12, 12, 1.2] }, { c: [19, 12, 1.2] }],
    sidebar: [{ r: [3, 4, 18, 16, 2] }, "M9 4v16"],
    sun: [{ c: [12, 12, 4] }, "M12 2v2", "M12 20v2", "M4.9 4.9l1.4 1.4", "M17.7 17.7l1.4 1.4", "M2 12h2", "M20 12h2", "M4.9 19.1l1.4-1.4", "M17.7 6.3l1.4-1.4"],
    moon: ["M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"],
    shield: ["M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z", "M8.5 12l2.5 2.5 4.5-4.5"],
    shieldX: ["M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z", "M9.5 9.5l5 5", "M14.5 9.5l-5 5"],
    lock: [{ r: [5, 11, 14, 10, 2] }, "M8 11V7a4 4 0 0 1 8 0v4"],
    word: [{ r: [4, 3, 16, 18, 2] }, "M7.5 8l1.5 8 3-6 3 6 1.5-8"],
    sheet: [{ r: [4, 3, 16, 18, 2] }, "M4 9h16", "M4 15h16", "M10 3v18"],
    slides: [{ r: [3, 4, 18, 13, 2] }, "M12 17v4", "M8 21h8", "M8 9h8", "M8 12h5"],
    file: ["M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z", "M14 3v5h5"],
    user: [{ c: [12, 8, 4] }, "M4 21a8 8 0 0 1 16 0"],
    users: [{ c: [9, 8, 3.5] }, "M2.5 20a6.5 6.5 0 0 1 13 0", "M16 4.5a3.5 3.5 0 0 1 0 7", "M18 14a6.5 6.5 0 0 1 3.5 6"],
    key: [{ c: [8, 15, 4] }, "M11 12l9-9", "M17 6l3 3", "M15 8l2 2"],
    comment: ["M21 12a8 8 0 0 1-11.6 7.1L4 20l1.2-4.3A8 8 0 1 1 21 12z"],
    merge: [{ c: [6, 5, 2.2] }, { c: [6, 19, 2.2] }, { c: [18, 12, 2.2] }, "M6 7.2v9.6", "M6 7.5c0 3 4 4.5 9.8 4.5"],
    revert: ["M9 14L4 9l5-5", "M4 9h10.5a5.5 5.5 0 0 1 0 11H11"],
    cherry: [{ c: [7, 17, 3] }, { c: [17, 17, 3] }, "M7 14c1-5 4-8 9-10", "M17 14c-.5-4-1-7-1-10"],
    compare: ["M8 3v13", "M4 12l4 4 4-4", "M16 21V8", "M12 12l4-4 4 4"],
    graph: [{ c: [6, 6, 2] }, { c: [6, 18, 2] }, { c: [18, 12, 2] }, "M6 8v8", "M8 6.5c5 0 8 2 8.2 3.6", "M8 17.5c5 0 8-2 8.2-3.6"],
    star: ["M12 3l2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9z"],
    eye: ["M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z", { c: [12, 12, 3] }],
    link: ["M10 14a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1", "M14 10a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1"],
    external: ["M14 4h6v6", "M20 4l-9 9", "M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"],
    trash: ["M4 7h16", "M10 11v6", "M14 11v6", "M5 7l1 13a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1l1-13", "M9 7V4h6v3"],
    edit: ["M4 20h4L19 9l-4-4L4 16z", "M13.5 6.5l4 4"],
    info: [{ c: [12, 12, 9] }, "M12 11v6", "M12 7.5v.5"],
    warning: ["M12 3l10 18H2z", "M12 10v5", "M12 18v.5"],
    issue: [{ c: [12, 12, 9] }, { c: [12, 12, 1.5] }],
    send: ["M4 12l16-8-6 16-2.5-6.5z"],
    stop: [{ r: [6, 6, 12, 12, 2] }],
    paperclip: ["M20 11.5l-8.3 8.3a5 5 0 0 1-7-7L13 4.5a3.3 3.3 0 0 1 4.7 4.7l-8.2 8.2a1.7 1.7 0 0 1-2.4-2.4l7.6-7.6"],
    skill: ["M4 5h16v14H4z", "M8 9l3 3-3 3", "M13 15h3"],
    keyboard: [{ r: [2, 6, 20, 12, 2] }, "M6 10h.5", "M10 10h.5", "M14 10h.5", "M18 10h.5", "M7 14h10"],
    palette: [{ c: [12, 12, 9] }, { c: [8, 10, 1.2] }, { c: [12, 7.5, 1.2] }, { c: [16, 10, 1.2] }, "M12 21a2 2 0 0 1 0-4h2a3 3 0 0 0 3-3"],
    bell: ["M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9", "M10.3 21a1.9 1.9 0 0 0 3.4 0"],
    server: [{ r: [3, 4, 18, 7, 2] }, { r: [3, 13, 18, 7, 2] }, "M7 7.5h.5", "M7 16.5h.5"],
    accessibility: [{ c: [12, 4.5, 1.8] }, "M5 8.5l7 1.5 7-1.5", "M12 10v4l-3 7", "M12 14l3 7"],
    prompt: [{ c: [12, 12, 9] }, "M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6v.6", "M12 17v.5"],
    advanced: ["M4 6h10", "M18 6h2", { c: [16, 6, 2] }, "M4 12h2", "M10 12h10", { c: [8, 12, 2] }, "M4 18h8", "M16 18h4", { c: [14, 18, 2] }],
    admin: ["M12 3l8 3v6c0 4.5-3.4 8.3-8 9-4.6-.7-8-4.5-8-9V6z"],
    layers: ["M12 3l9 5-9 5-9-5z", "M3 13l9 5 9-5"],
    split: [{ r: [3, 4, 18, 16, 2] }, "M12 4v16"],
    unified: [{ r: [3, 4, 18, 16, 2] }, "M3 12h18"],
    filter: ["M4 5h16l-6 7.5V19l-4 2v-8.5z"],
    pin: ["M9 4h6l-1 6 3 3H7l3-3z", "M12 13v8"],
    dot: [{ c: [12, 12, 4] }],
    globe: [{ c: [12, 12, 9] }, "M3 12h18", "M12 3a14 14 0 0 1 0 18", "M12 3a14 14 0 0 0 0 18"]
  };
  function icon(name2, size = 16, cls = "icon") {
    let shapes = I[name2] ?? [];
    return s(
      "svg",
      { class: cls, width: size, height: size, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", "stroke-width": 1.75, "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true", focusable: "false" },
      shapes.map((sh) => typeof sh == "string" ? s("path", { d: sh }) : "c" in sh ? s("circle", { cx: sh.c[0], cy: sh.c[1], r: sh.c[2] }) : s("rect", { x: sh.r[0], y: sh.r[1], width: sh.r[2], height: sh.r[3], rx: sh.r[4] }))
    );
  }
  var ICON_NAMES = Object.keys(I);

  // client/src/lib/format.ts
  var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"], pad = (n) => String(n).padStart(2, "0");
  function formatDate(ms, p2) {
    let d = new Date(ms), y = d.getUTCFullYear(), m = d.getUTCMonth(), day = d.getUTCDate();
    return p2.dateFormat === "iso" ? `${y}-${pad(m + 1)}-${pad(day)}` : p2.dateFormat === "dmy" ? `${pad(day)}/${pad(m + 1)}/${y}` : `${MONTHS[m]} ${day}, ${y}`;
  }
  function formatTime(ms, p2) {
    let d = new Date(ms), h2 = d.getUTCHours(), min = d.getUTCMinutes();
    return p2.timeFormat === "24h" ? `${pad(h2)}:${pad(min)}` : `${h2 % 12 === 0 ? 12 : h2 % 12}:${pad(min)} ${h2 < 12 ? "am" : "pm"}`;
  }
  function relative(ms, now, p2) {
    if (p2.absoluteDates) return `${formatDate(ms, p2)} ${formatTime(ms, p2)}`;
    let s2 = Math.max(0, Math.round((now - ms) / 1e3));
    if (s2 < 45) return "just now";
    let min = Math.round(s2 / 60);
    if (min < 60) return `${min} min ago`;
    let hr = Math.round(min / 60);
    if (hr < 24) return `${hr} h ago`;
    let d = Math.round(hr / 24);
    return d === 1 ? "yesterday" : d < 14 ? `${d} d ago` : formatDate(ms, p2);
  }
  function age(ms, now) {
    let s2 = Math.max(0, Math.round((now - ms) / 1e3));
    if (s2 < 60) return "now";
    let m = Math.floor(s2 / 60);
    if (m < 60) return `${m}m`;
    let h2 = Math.floor(m / 60);
    if (h2 < 24) return `${h2}h`;
    let d = Math.floor(h2 / 24);
    return d < 14 ? `${d}d` : `${Math.floor(d / 7)}w`;
  }
  function initials(name2) {
    let parts = name2.trim().split(/\s+/);
    return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase();
  }

  // client/src/ui/controls.ts
  function button(label, o = {}) {
    return h(
      "button",
      {
        type: o.type ?? "button",
        class: cx("btn", `btn-${o.variant ?? "secondary"}`, o.size === "sm" && "btn-sm", o.class),
        disabled: o.disabled,
        title: o.title,
        id: o.id,
        "aria-pressed": o.pressed === void 0 ? void 0 : String(o.pressed),
        onClick: o.onClick
      },
      o.icon ? icon(o.icon, 15) : null,
      label === "" ? null : h("span", null, label)
    );
  }
  function iconButton(name2, label, onClick, o = {}) {
    return h("button", { type: "button", class: cx("icon-btn", o.active && "is-active", o.class), "aria-label": label, title: label, disabled: o.disabled, id: o.id, onClick }, icon(name2, o.size ?? 16));
  }
  function toggle(on, onChange, label, o = {}) {
    return h(
      "button",
      {
        type: "button",
        role: "switch",
        class: cx("switch", on && "is-on"),
        "aria-checked": String(on),
        "aria-label": label,
        disabled: o.disabled,
        id: o.id,
        onClick: () => onChange(!on)
      },
      h("span", { class: "switch-thumb" })
    );
  }
  function checkbox(checked, onChange, label, o = {}) {
    return h("input", {
      type: "checkbox",
      class: "check",
      checked: checked === !0,
      indeterminate: checked === "mixed",
      "aria-label": label,
      id: o.id,
      disabled: o.disabled,
      onClick: (e) => e.stopPropagation(),
      onChange: (e) => onChange(e.target.checked)
    });
  }
  function select(value, options, onChange, o = {}) {
    return h(
      "select",
      { class: cx("select", o.class), id: o.id, "aria-label": o.label, disabled: o.disabled, onChange: (e) => onChange(e.target.value) },
      options.map(([v, l]) => h("option", { value: v, selected: v === value }, l))
    );
  }
  function input(value, onInput, o = {}) {
    return h("input", {
      type: o.type ?? "text",
      class: cx("input", o.class),
      value,
      id: o.id,
      placeholder: o.placeholder,
      "aria-label": o.label ?? o.placeholder,
      maxlength: o.maxlength,
      disabled: o.disabled,
      spellcheck: "false",
      autocomplete: "off",
      "data-autofocus": o.autofocus ? "1" : void 0,
      onInput: (e) => onInput(e.target.value),
      onKeydown: (e) => {
        e.key === "Enter" && o.onEnter && (e.preventDefault(), o.onEnter());
      }
    });
  }
  function textarea(value, onInput, o = {}) {
    let t = h("textarea", {
      class: cx("textarea", o.class),
      id: o.id,
      placeholder: o.placeholder,
      "aria-label": o.label ?? o.placeholder,
      rows: o.rows ?? 3,
      maxlength: o.maxlength,
      onInput: (e) => onInput(e.target.value),
      onKeydown: o.onKeydown
    });
    return t.value = value, t;
  }
  function pill(text, tone = "neutral", o = {}) {
    return h("span", { class: cx("pill", `tone-${tone}`, o.mono !== !1 && "mono"), title: o.title }, h("span", { class: "pill-dot" }), text);
  }
  function badge(text, tone = "neutral") {
    return h("span", { class: cx("badge", `tone-${tone}`) }, text);
  }
  function avatar(name2, agent2 = !1, size = "sm") {
    return h("span", { class: cx("avatar", `avatar-${size}`, agent2 && "is-agent"), title: name2, "aria-hidden": "true" }, agent2 ? icon("agent", size === "sm" ? 12 : 14) : initials(name2));
  }
  function row(title, description, control, o = {}) {
    return h(
      "div",
      { class: cx("srow", o.stack && "srow-stack"), id: o.id },
      h("div", { class: "srow-text" }, h("div", { class: "srow-title" }, title, o.fixed ? h("span", { class: "fixed-tag" }, "fixed") : null), description ? h("div", { class: "srow-desc" }, description) : null),
      control ? h("div", { class: "srow-control" }, control) : null
    );
  }
  function rows2(...children) {
    return h("div", { class: "rows" }, ...children);
  }
  function section(title, description, ...children) {
    return h("section", { class: "ssection" }, h("h3", { class: "ssection-title" }, title), description ? h("p", { class: "ssection-desc" }, description) : null, ...children);
  }
  function empty(iconName, title, text, action) {
    return h("div", { class: "empty" }, h("div", { class: "empty-icon" }, icon(iconName, 22)), h("div", { class: "empty-title" }, title), h("div", { class: "empty-text" }, text), action ?? null);
  }
  function notice(tone, ...children) {
    return h("div", { class: cx("notice", `notice-${tone}`), role: tone === "error" ? "alert" : void 0 }, icon(tone === "success" ? "check" : tone === "info" ? "info" : "warning", 15), h("div", null, ...children));
  }
  function untrusted(label, text) {
    return h("div", { class: "untrusted" }, h("div", { class: "untrusted-label" }, label), h("div", { class: "untrusted-text" }, text));
  }
  function segmented(value, options, onChange, label) {
    return h(
      "div",
      { class: "segmented", role: "radiogroup", "aria-label": label },
      options.map(
        ([v, l, ic]) => h("button", { type: "button", role: "radio", "aria-checked": String(v === value), class: cx("seg", v === value && "is-on"), onClick: () => onChange(v) }, ic ? icon(ic, 14) : null, l ? h("span", null, l) : null)
      )
    );
  }
  function tabs(value, options, onChange, label) {
    return h(
      "div",
      { class: "tabs", role: "tablist", "aria-label": label },
      options.map(
        ([v, l, n]) => h("button", { type: "button", role: "tab", "aria-selected": String(v === value), class: cx("tab", v === value && "is-on"), onClick: () => onChange(v) }, l, n !== void 0 ? h("span", { class: "tab-count" }, String(n)) : null)
      )
    );
  }
  function field(label, control, hint) {
    return h("label", { class: "field" }, h("span", { class: "field-label" }, label), control, hint ? h("span", { class: "field-hint" }, hint) : null);
  }
  function hash(hex, n = 8) {
    return h("code", { class: "hash", title: hex }, hex.slice(0, n));
  }

  // client/src/ui/layer.ts
  var menuEl = null, menuReturn = null;
  function closeMenu() {
    menuEl && (menuEl.remove(), menuEl = null, document.removeEventListener("pointerdown", outside, !0), menuReturn && document.contains(menuReturn) && menuReturn.focus(), menuReturn = null);
  }
  function outside(e) {
    menuEl && !menuEl.contains(e.target) && closeMenu();
  }
  function openMenu(anchor, items, o = {}) {
    closeMenu();
    let r = anchor.getBoundingClientRect(), list2 = h(
      "div",
      { class: "menu-list", role: "menu" },
      items.map((it) => it.separator ? h("div", { class: "menu-sep", role: "separator" }) : it.heading ? h("div", { class: "menu-heading" }, it.heading) : h(
        "button",
        {
          type: "button",
          role: it.checked === void 0 ? "menuitem" : "menuitemcheckbox",
          "aria-checked": it.checked === void 0 ? void 0 : String(it.checked),
          class: cx("menu-item", it.danger && "is-danger", it.disabled && "is-disabled"),
          "aria-disabled": it.disabled ? "true" : void 0,
          title: it.note,
          onClick: () => {
            it.disabled || (closeMenu(), it.onSelect?.());
          }
        },
        h("span", { class: "menu-icon" }, it.checked ? icon("check", 14) : it.icon ? icon(it.icon, 14) : null),
        h("span", { class: "menu-label" }, h("span", null, it.label), it.note ? h("span", { class: "menu-note" }, it.note) : null),
        it.shortcut ? h("span", { class: "menu-shortcut" }, it.shortcut) : null
      ))
    ), el = h("div", { class: cx("menu", o.className), css: { width: `${o.width ?? 280}px` } }, o.extra ?? null, list2), layer2 = document.getElementById("menus");
    if (!layer2) return;
    layer2.appendChild(el);
    let width = o.width ?? 280, left = o.align === "right" ? r.right - width : r.left;
    left = Math.max(8, Math.min(left, window.innerWidth - width - 8));
    let top = r.bottom + 4, height = el.getBoundingClientRect().height;
    top + height > window.innerHeight - 8 && (top = Math.max(8, r.top - height - 4)), el.style.setProperty("left", `${left}px`), el.style.setProperty("top", `${top}px`), menuEl = el, menuReturn = anchor, document.addEventListener("pointerdown", outside, !0), el.addEventListener("keydown", (e) => {
      let buttons = Array.from(el.querySelectorAll(".menu-item:not(.is-disabled)")), i = buttons.indexOf(document.activeElement);
      e.key === "Escape" ? (e.preventDefault(), e.stopPropagation(), closeMenu()) : e.key === "ArrowDown" ? (e.preventDefault(), buttons[(i + 1) % buttons.length]?.focus()) : e.key === "ArrowUp" && (e.preventDefault(), buttons[(i - 1 + buttons.length) % buttons.length]?.focus());
    }), el.querySelector("input, .menu-item:not(.is-disabled)")?.focus();
  }
  var dialog = null, dialogFresh = !1;
  function openDialog(spec) {
    closeMenu(), dialog = spec, dialogFresh = !0, render();
  }
  function closeDialog() {
    let d = dialog;
    dialog = null, d?.onClose?.(), render();
  }
  function dialogOpen() {
    return dialog !== null;
  }
  function confirmDialog(title, text, confirmLabel, onConfirm, o = {}) {
    openDialog({
      title,
      plain: !0,
      width: 480,
      body: () => h("div", { class: "dialog-text" }, text),
      actions: () => [
        button("Cancel", { onClick: closeDialog }),
        button(confirmLabel, {
          variant: o.danger ? "danger" : "primary",
          id: "dialog-confirm",
          onClick: () => {
            closeDialog(), onConfirm();
          }
        })
      ]
    });
  }
  var paletteOpen = !1, paletteQuery = "", paletteIndex = 0, commandSource = () => [];
  function setCommands(fn) {
    commandSource = fn;
  }
  function openPalette(query = "") {
    closeMenu(), paletteOpen = !0, paletteQuery = query, paletteIndex = 0, dialogFresh = !0, render();
  }
  function closePalette() {
    paletteOpen = !1, render();
  }
  function paletteIsOpen() {
    return paletteOpen;
  }
  function score(c, q) {
    if (!q) return 1;
    let hay = `${c.label} ${c.group} ${c.keywords ?? ""}`.toLowerCase(), words2 = q.toLowerCase().split(/\s+/).filter(Boolean), s2 = 0;
    for (let w of words2) {
      let i = hay.indexOf(w);
      if (i < 0) return 0;
      s2 += i === 0 ? 3 : c.label.toLowerCase().includes(w) ? 2 : 1;
    }
    return s2;
  }
  function paletteView() {
    var _a;
    let list2 = commandSource().map((c) => ({ c, s: score(c, paletteQuery) })).filter((x) => x.s > 0).sort((a, b) => b.s - a.s).slice(0, 60).map((x) => x.c);
    paletteIndex >= list2.length && (paletteIndex = Math.max(0, list2.length - 1));
    let groups = {};
    for (let c of list2) (groups[_a = c.group] ?? (groups[_a] = [])).push(c);
    let n = -1, run = (c) => {
      c.disabled || (paletteOpen = !1, render(), c.run());
    }, inputEl = h("input", {
      class: "palette-input",
      id: "palette-input",
      value: paletteQuery,
      placeholder: "Type a command, a file or a change request",
      "aria-label": "Command",
      spellcheck: "false",
      autocomplete: "off",
      "data-autofocus": "1",
      onInput: (e) => {
        paletteQuery = e.target.value, paletteIndex = 0, render();
      },
      onKeydown: (e) => {
        if (e.key === "ArrowDown")
          e.preventDefault(), paletteIndex = Math.min(list2.length - 1, paletteIndex + 1), render();
        else if (e.key === "ArrowUp")
          e.preventDefault(), paletteIndex = Math.max(0, paletteIndex - 1), render();
        else if (e.key === "Enter") {
          e.preventDefault();
          let c = list2[paletteIndex];
          c && run(c);
        } else e.key === "Escape" && (e.preventDefault(), closePalette());
      }
    });
    return h(
      "div",
      { class: "overlay", onPointerdown: (e) => e.target === e.currentTarget && closePalette() },
      h(
        "div",
        { class: "palette", role: "dialog", "aria-label": "Command palette" },
        h("div", { class: "palette-search" }, icon("search", 16), inputEl),
        h(
          "div",
          { class: "palette-list", "data-scroll-key": "palette" },
          list2.length === 0 ? h("div", { class: "palette-empty" }, "Nothing matches.") : null,
          Object.keys(groups).map((g) => [
            h("div", { class: "palette-group" }, g),
            groups[g].map((c) => {
              n++;
              let i = n;
              return h(
                "button",
                { type: "button", class: cx("palette-item", i === paletteIndex && "is-active", c.disabled && "is-disabled"), "aria-disabled": c.disabled ? "true" : void 0, onClick: () => run(c), onPointermove: () => {
                  paletteIndex !== i && (paletteIndex = i, render());
                } },
                h("span", { class: "palette-icon" }, icon(c.icon ?? "chevronRight", 15)),
                h("span", { class: "palette-label" }, c.label),
                c.shortcut ? kbd(c.shortcut) : null
              );
            })
          ])
        ),
        h("div", { class: "palette-foot" }, kbd("Up"), kbd("Down"), h("span", null, "to move"), kbd("Enter"), h("span", null, "to run"), kbd("Esc"), h("span", null, "to close"))
      )
    );
  }
  function toasts() {
    return h(
      "div",
      { class: "toasts", "aria-live": "polite" },
      state.ui.toasts.map((t) => h("div", { class: cx("toast", `toast-${t.kind}`) }, h("span", { class: "toast-dot" }), h("div", null, h("div", { class: "toast-title" }, t.title), t.text ? h("div", { class: "toast-text" }, t.text) : null)))
    );
  }
  function dialogView(d) {
    let box = h(
      "div",
      { class: cx("dialog", d.plain && "is-plain"), role: "dialog", "aria-modal": "true", "aria-labelledby": "dialog-title", css: { width: `min(${d.width ?? 620}px, calc(100vw - 32px))` } },
      h("div", { class: "dialog-head" }, h("h2", { id: "dialog-title" }, d.title), h("button", { type: "button", class: "icon-btn", "aria-label": "Close", onClick: closeDialog }, icon("x", 16))),
      h("div", { class: "dialog-body", "data-scroll-key": "dialog" }, d.body()),
      d.actions ? h("div", { class: "dialog-actions" }, d.actions()) : null
    );
    return box.addEventListener("keydown", (e) => {
      e.key === "Escape" && (e.preventDefault(), closeDialog());
    }), h("div", { class: "overlay", onPointerdown: (e) => e.target === e.currentTarget && closeDialog() }, box);
  }
  function renderLayer(layer2) {
    if (clear(layer2), dialog && layer2.appendChild(dialogView(dialog)), paletteOpen && layer2.appendChild(paletteView()), layer2.appendChild(toasts()), dialogFresh) {
      dialogFresh = !1;
      let target = layer2.querySelector("[data-autofocus]") ?? layer2.querySelector(".dialog-body input, .dialog-body textarea, .dialog-body select, #dialog-confirm, .dialog-actions button");
      requestAnimationFrame(() => target?.focus());
    }
  }

  // client/src/views/common.ts
  function currentProject() {
    return project(state.db, state.ui.projectId);
  }
  function currentBranch() {
    return branchOf();
  }
  function currentDraft() {
    let b = currentBranch();
    return b === "main" ? void 0 : draft(state.db, b);
  }
  function branchLabel(branch = currentBranch()) {
    return branchName(state.db, branch);
  }
  function personName(id) {
    return id === state.db.me ? "You" : person(state.db, id).name;
  }
  function isAgent(id) {
    return !!state.db.people.find((p2) => p2.id === id)?.agent;
  }
  var KIND_ICON = { document: "word", spreadsheet: "sheet", presentation: "slides" }, KIND_LABEL = { document: "Document", spreadsheet: "Spreadsheet", presentation: "Presentation" };
  function fileIcon(kind, size = 15) {
    return icon(KIND_ICON[kind], size, `icon kind-${kind}`);
  }
  var ROLE_LABEL = { owner: "Owner", manager: "Manager", editor: "Editor", approver: "Reviewer", reviewer: "Contributor", viewer: "Viewer" }, ROLE_SUMMARY = {
    owner: "Everything, including transferring the project and deleting it.",
    manager: "Manages people, rules and settings; edits and merges.",
    editor: "Edits main where it is not protected, opens drafts and change requests, merges where the rules allow.",
    approver: "Approves or requests changes on change requests; comments.",
    reviewer: "Comments and suggests through drafts; cannot approve.",
    viewer: "Reads files and their history."
  };
  function when(ms) {
    return relative(ms, Date.now(), state.prefs);
  }
  function short(ms) {
    return age(ms, Date.now());
  }
  function rev(c) {
    return `${c.branch === "main" ? "r" : "d"}${c.seq}`;
  }
  function branchContext(branch = currentBranch(), projectId = state.ui.projectId) {
    let p2 = project(state.db, projectId), d = branch === "main" ? void 0 : draft(state.db, branch), req = d ? state.db.requests.find((r) => r.draftId === d.id && r.status === "open") : void 0;
    return {
      kind: d ? "draft" : "main",
      state: d?.state,
      behind: d ? behind(state.db, d) : 0,
      ahead: aheadOf(state.db, projectId, branch),
      uncommitted: working(state.db, projectId, branch).length,
      role: p2.role,
      requestSatisfied: req ? reviewState(state.db, req).satisfied : !1,
      protectedMain: p2.rules.protectMain,
      mine: d ? d.authorId === state.db.me : !0
    };
  }
  function commitBase(c) {
    let later = branchCommits(state.db, c.docId, c.branch).filter((x) => x.seq >= c.seq).reverse(), blocks = blocksOf(state.db, c.docId, c.branch);
    for (let x of later) blocks = applyEdits(blocks, invert(x.edits));
    return blocks;
  }
  function requestTone(status) {
    return status === "open" ? "success" : status === "merged" ? "primary" : status === "declined" ? "error" : "neutral";
  }
  function draftTone(s2) {
    return s2 === "private" ? "neutral" : s2 === "in_review" ? "info" : s2 === "merged" ? "primary" : "neutral";
  }
  function draftWord(s2) {
    return s2 === "in_review" ? "in review" : s2;
  }
  function act(fn, success) {
    try {
      let out = fn();
      return (success || typeof out == "string") && toast("success", success ?? out), render(), !0;
    } catch (e) {
      return toast("error", e instanceof Error ? e.message : String(e)), render(), !1;
    }
  }
  function docOf(id) {
    return doc(state.db, id);
  }
  function verifyPill(commitId) {
    let v = state.ui.verified[commitId];
    return v ? v.ok ? pill("verified", "success", { title: "Chain hash recomputed and equal" }) : pill("broken", "error") : pill("unchecked", "neutral", { title: 'Not verified in this session. Use "Verify hash chain".' });
  }
  function verifyIcon(commitId) {
    let v = state.ui.verified[commitId], name2 = v ? v.ok ? "shield" : "shieldX" : "shield", cls = v ? v.ok ? "vf-ok" : "vf-bad" : "vf-unchecked", label = v ? v.ok ? "Chain verified" : "Chain broken here" : "Chain not verified yet";
    return h("span", { class: `vf ${cls}`, title: label, "aria-label": label }, icon(name2, 14));
  }
  function runVerify(docId) {
    let res = verify(state.db, docId), ok = 0, bad = 0;
    for (let r of res)
      state.ui.verified[r.commitId] = { ok: r.ok, at: Date.now() }, r.ok ? ok++ : bad++;
    return { ok, bad };
  }
  function viewHeader(title, subtitle, ...actions) {
    return h("div", { class: "view-head" }, h("div", null, h("h1", { class: "view-title" }, title), subtitle ? h("div", { class: "view-sub" }, subtitle) : null), actions.length ? h("div", { class: "view-actions" }, actions) : null);
  }

  // client/src/views/actions.ts
  function switchProject(id) {
    state.ui.projectId = id, state.ui.changes.selected = null, state.ui.history.selected = null, state.ui.requests.selected = null, state.ui.documents.selected = null, render();
  }
  function switchBranch(branch) {
    let from = currentBranch();
    if (from === branch) return;
    let pending = working(state.db, state.ui.projectId, from).length, doSwitch = () => {
      state.ui.branchByProject[state.ui.projectId] = branch, state.ui.changes.selected = null, state.ui.history.selected = null, render();
    };
    pending && state.prefs.prompts.switchWithChanges === "ask" ? confirmDialog(
      "Switch branch",
      h("p", null, `You have ${pending} captured change${pending === 1 ? "" : "s"} on ${branchLabel(from)}. They stay on ${branchLabel(from)} and come back when you switch back. Dynodoc does not carry captured changes between a draft and main.`),
      `Switch to ${branchLabel(branch)}`,
      doSwitch
    ) : doSwitch();
  }
  function fetchNow() {
    state.ui.fetching || (state.ui.fetching = !0, render(), setTimeout(() => {
      state.ui.fetching = !1, state.db.lastFetched = Date.now();
      let next = INCOMING[state.db.incoming];
      next ? (state.db.incoming += 1, toast("info", "Fetched", next.apply(state.db))) : toast("success", "Up to date", "Nothing new on the server."), render();
    }, 650));
  }
  function sync() {
    if (!aheadOf(state.db, state.ui.projectId, currentBranch())) return fetchNow();
    state.ui.fetching = !0, render(), setTimeout(() => {
      state.ui.fetching = !1;
      let n = push(state.db, state.ui.projectId, currentBranch());
      state.db.lastFetched = Date.now(), log(state.db, { actorId: state.db.me, projectId: state.ui.projectId, kind: "commit", summary: `pushed ${n} commit${n === 1 ? "" : "s"} to ${branchLabel()}` }), toast("success", `Pushed ${n} commit${n === 1 ? "" : "s"} to ${branchLabel()}`, "Signed with this computer’s key and added to the server’s history."), render();
    }, 700);
  }
  function newDraftDialog(docId) {
    let docs = docsOf(state.db, state.ui.projectId), target = docId ?? docs[0]?.id ?? "", name2 = "", error = "", create = () => {
      try {
        let d = newDraft(state.db, target, name2);
        closeDialog(), state.ui.branchByProject[state.ui.projectId] = d.id, toast("success", `Created the draft ${d.name}`, `Based on main r${d.baseSeq} of ${docOf(target).title}.`), render();
      } catch (e) {
        error = e instanceof Error ? e.message : String(e), render();
      }
    };
    openDialog({
      title: "New draft",
      width: 520,
      body: () => [
        h("p", { class: "dialog-text" }, "A draft is a private branch of one file, based on the latest main. Commit to it, then send it for review."),
        field("Name", input(name2, (v) => {
          name2 = v;
        }, { id: "draft-name", placeholder: "remote-work-update", autofocus: !0, onEnter: create, maxlength: 63 })),
        field("File", select(target, docs.map((d) => [d.id, `${d.filename} (main r${mainSeq(state.db, d.id)})`]), (v) => {
          target = v;
        }, { id: "draft-file", label: "File" })),
        error ? notice("error", error) : null
      ],
      actions: () => [button("Cancel", { onClick: closeDialog }), button("Create draft", { variant: "primary", onClick: create })]
    });
  }
  function sendForReviewDialog(draftId = currentBranch()) {
    let d = draft(state.db, draftId);
    if (!d) return;
    let name2 = "", note = "", edits2 = draftEdits(state.db, d);
    openDialog({
      title: "Send for review",
      width: 560,
      body: () => [
        h("p", { class: "dialog-text" }, `Opens a change request from ${d.name} into main for ${docOf(d.docId).title}: ${edits2.length} change${edits2.length === 1 ? "" : "s"}. Reviewers see it in Change requests; a person merges it when the project's rules are met.`),
        field("Title", input(name2, (v) => {
          name2 = v;
        }, { id: "cr-title", placeholder: d.name, autofocus: !0, maxlength: 120 })),
        field("Message", textarea(note, (v) => {
          note = v;
        }, { id: "cr-note", rows: 4, placeholder: "What changed and why" }))
      ],
      actions: () => [
        button("Cancel", { onClick: closeDialog }),
        button("Open change request", {
          variant: "primary",
          onClick: () => {
            act(() => {
              let r = sendForReview(state.db, d.id, name2, note);
              return state.ui.requests.selected = r.id, state.ui.requests.filter = "open", `Opened change request #${r.number}`;
            }) && (closeDialog(), go("requests"));
          }
        })
      ]
    });
  }
  function rebaseDialog(draftId = currentBranch(), mode = "rebase") {
    let d = draft(state.db, draftId);
    if (!d) return;
    let conflicts2 = conflictsOf(state.db, d), choices = {}, behind2 = behind(state.db, d);
    openDialog({
      title: mode === "update" ? "Update from main" : "Rebase onto main",
      width: 680,
      body: () => [
        h("p", { class: "dialog-text" }, `${d.name} is based on main r${d.baseSeq}; main is at r${mainSeq(state.db, d.docId)} (${behind2} newer revision${behind2 === 1 ? "" : "s"}). ${mode === "update" ? "Bringing main in" : "Rebasing"} replays your draft on the latest main.`),
        conflicts2.length ? [
          notice("warning", `${conflicts2.length} block${conflicts2.length === 1 ? "" : "s"} changed on both sides. Choose which version to keep for each.`),
          h(
            "div",
            { class: "conflicts" },
            conflicts2.map(
              (c) => h(
                "div",
                { class: "conflict" },
                h("div", { class: "conflict-label" }, c.label),
                h("div", { class: "conflict-ancestor" }, h("span", { class: "muted" }, "Before both: "), c.ancestor),
                h(
                  "div",
                  { class: "conflict-choices" },
                  ["team", "draft"].map(
                    (side) => h(
                      "label",
                      { class: `conflict-choice ${choices[c.blockId] === side ? "is-on" : ""}` },
                      h("input", { type: "radio", name: `cf-${c.blockId}`, checked: choices[c.blockId] === side, onChange: () => {
                        choices[c.blockId] = side, render();
                      } }),
                      h("span", { class: "conflict-side" }, side === "team" ? "Team's newer version" : "Your version"),
                      h("span", { class: "conflict-text" }, side === "team" ? c.team : c.draft)
                    )
                  )
                )
              )
            )
          )
        ] : notice("success", "No conflicts: none of the blocks your draft changes were changed on main.")
      ],
      actions: () => [
        button("Cancel", { onClick: closeDialog }),
        button(mode === "update" ? "Update draft" : "Rebase draft", {
          variant: "primary",
          disabled: conflicts2.some((c) => !choices[c.blockId]),
          onClick: () => {
            act(() => (rebase(state.db, d.id, choices), `${d.name} now builds on main r${d.baseSeq}`)) && closeDialog();
          }
        })
      ]
    });
  }
  function withdrawDraft(draftId = currentBranch()) {
    let d = draft(state.db, draftId);
    if (!d) return;
    let run = () => {
      act(() => withdraw(state.db, d.id), `Withdrew ${d.name}`) && (currentBranch() === d.id && (state.ui.branchByProject[state.ui.projectId] = "main"), render());
    };
    if (!state.prefs.prompts.withdraw) return run();
    confirmDialog("Withdraw draft", h("p", null, `Withdraw ${d.name}? It is closed and kept in history as withdrawn; its open change request is withdrawn too. This cannot be undone, but you can start a new draft.`), "Withdraw draft", run, { danger: !0 });
  }
  function discardAll() {
    let list2 = working(state.db, state.ui.projectId, currentBranch());
    if (!list2.length) return;
    let run = () => act(() => {
      discard(state.db, list2.map((w) => w.id));
    }, `Discarded ${list2.length} change${list2.length === 1 ? "" : "s"}`);
    if (!state.prefs.prompts.discard) return void run();
    confirmDialog("Discard all changes", h("p", null, `Discard all ${list2.length} captured changes on ${branchLabel()}? They were never committed, so they cannot be brought back from history. The documents in your editor keep their text.`), "Discard changes", run, { danger: !0 });
  }
  function compareWithMain(draftId = currentBranch()) {
    draftId !== "main" && (state.ui.history.mode = "compare", state.ui.history.compareBranch = draftId, go("history"));
  }
  function openRequestFor(draftId = currentBranch()) {
    let r = state.db.requests.find((x) => x.draftId === draftId && x.status === "open");
    r && (state.ui.requests.selected = r.id, state.ui.requests.filter = "open", go("requests"));
  }
  function runBranchAction(id) {
    let ctx = branchContext(), item = branchMenu(ctx).find((a) => a.id === id);
    if (!item || !item.enabled) {
      item && toast("info", item.label.replace("…", ""), item.note);
      return;
    }
    switch (id) {
      case "new":
        return newDraftDialog(currentDraft()?.docId);
      case "delete":
        return withdrawDraft();
      case "discard":
        return discardAll();
      case "update":
        return rebaseDialog(currentBranch(), "update");
      case "rebase":
        return rebaseDialog(currentBranch(), "rebase");
      case "compare":
        return compareWithMain();
      case "merge":
      case "view":
        return openRequestFor();
      case "request":
        return sendForReviewDialog();
      default:
        return;
    }
  }
  function branchMenuItems() {
    let items = [];
    for (let a of branchMenu(branchContext()))
      a.group && items.push({ label: "", separator: !0 }), items.push({ label: a.label, shortcut: a.shortcut, disabled: !a.enabled, note: a.enabled ? void 0 : a.note, onSelect: () => runBranchAction(a.id) });
    return items;
  }
  function openBranchMenu(anchor) {
    openMenu(anchor, branchMenuItems(), { width: 320 });
  }
  var listFilter = "";
  function openProjectList(anchor) {
    listFilter = "";
    let build = () => {
      let q = listFilter.toLowerCase(), all = state.db.projects.filter((p2) => p2.name.toLowerCase().includes(q)), recent = [...all].sort((a, b) => b.updatedAt - a.updatedAt).slice(0, 2), item = (p2) => ({ label: p2.name, icon: "repo", checked: p2.id === state.ui.projectId ? !0 : void 0, note: `${docsOf(state.db, p2.id).length} files · ${p2.private ? "private" : "shared"}`, onSelect: () => switchProject(p2.id) });
      return [{ label: "", heading: "Recent" }, ...recent.map(item), { label: "", heading: "All projects" }, ...all.map(item)];
    }, reopen = () => openMenu(anchor, build(), { width: 320, extra: filterBox("Filter projects", reopen), className: "menu-list-picker" });
    reopen();
  }
  function openBranchList(anchor) {
    listFilter = "";
    let build = () => {
      let q = listFilter.toLowerCase(), drafts2 = state.db.drafts.filter((d) => d.projectId === state.ui.projectId && d.name.includes(q)), mine = drafts2.filter((d) => d.authorId === state.db.me && (d.state === "private" || d.state === "in_review")), others = drafts2.filter((d) => d.authorId !== state.db.me && d.state === "in_review"), closed = drafts2.filter((d) => d.state === "merged" || d.state === "withdrawn"), row2 = (d) => ({
        label: d.name,
        icon: "branch",
        checked: currentBranch() === d.id ? !0 : void 0,
        note: `${docOf(d.docId).filename} · ${draftWord(d.state)}${behind(state.db, d) ? ` · ${behind(state.db, d)} behind` : ""}`,
        onSelect: () => switchBranch(d.id)
      }), out = [{ label: "", heading: "Default branch" }];
      return "main".includes(q) && out.push({ label: "main", icon: "branch", checked: currentBranch() === "main" ? !0 : void 0, note: "The team version", onSelect: () => switchBranch("main") }), mine.length && out.push({ label: "", heading: "Your drafts" }, ...mine.map(row2)), others.length && out.push({ label: "", heading: "In review by others" }, ...others.map(row2)), closed.length && out.push({ label: "", heading: "Closed" }, ...closed.slice(0, 6).map(row2)), out.push({ label: "", separator: !0 }, { label: "New draft…", icon: "plus", shortcut: "Ctrl+Shift+N", onSelect: () => runBranchAction("new") }), out;
    }, reopen = () => openMenu(anchor, build(), { width: 340, extra: filterBox("Filter branches", reopen), className: "menu-list-picker" });
    reopen();
  }
  function filterBox(placeholder, reopen) {
    return h("div", { class: "menu-filter" }, icon("search", 14), h("input", {
      class: "menu-filter-input",
      id: "menu-filter",
      value: listFilter,
      placeholder,
      "aria-label": placeholder,
      spellcheck: "false",
      onInput: (e) => {
        listFilter = e.target.value;
        let pos = e.target.selectionStart ?? listFilter.length;
        reopen();
        let f = document.getElementById("menu-filter");
        f?.focus(), f?.setSelectionRange(pos, pos);
      },
      onKeydown: (e) => {
        e.key === "ArrowDown" && (e.preventDefault(), document.querySelector(".menu .menu-item:not(.is-disabled)")?.focus());
      }
    }));
  }

  // client/src/views/activity.ts
  var KIND_ICON2 = {
    change_request_submitted: "request",
    change_request_reviewed: "check",
    change_request_merged: "merge",
    change_request_declined: "x",
    copy_detected: "documents",
    access_granted: "users",
    mention: "comment"
  };
  function open(n) {
    if (n.read = !0, n.projectId !== state.ui.projectId && switchProject(n.projectId), n.requestId) {
      let r = state.db.requests.find((x) => x.id === n.requestId);
      state.ui.requests.selected = n.requestId, state.ui.requests.filter = r?.status === "open" ? "open" : r?.status === "merged" ? "merged" : "closed", go("requests");
    } else n.docId ? (state.ui.documents.selected = n.docId, state.ui.documents.tab = n.kind === "copy_detected" ? "sync" : "overview", go("documents")) : (state.ui.projects.selected = n.projectId, go("projects"));
  }
  function unread() {
    return state.db.notifications.filter((n) => !n.read).length;
  }
  function activityView() {
    let ui = state.ui.activity, list2 = [...state.db.notifications].sort((a, b) => Number(a.read) - Number(b.read) || b.at - a.at), body = ui.tab === "notifications" ? list2.length ? h("div", { class: "list-card" }, list2.map((n) => h(
      "div",
      { class: cx("list-item notif", !n.read && "is-unread") },
      h("span", { class: "notif-icon" }, icon(KIND_ICON2[n.kind], 15)),
      avatar(personName(n.actorId), isAgent(n.actorId)),
      h("div", { class: "grow" }, h("div", null, h("strong", null, personName(n.actorId)), ` ${n.text}`, isAgent(n.actorId) ? [" ", badge("agent key", "info")] : null), h("div", { class: "muted small" }, `${project(state.db, n.projectId).name} · ${when(n.at)}`)),
      n.kind === "copy_detected" ? button("See copies", { size: "sm", onClick: () => open(n) }) : button(n.requestId ? "Open request" : "Open", { size: "sm", onClick: () => open(n) }),
      n.read ? null : button("Mark read", { size: "sm", variant: "ghost", onClick: () => {
        n.read = !0, render();
      } })
    ))) : empty("bell", "No notifications", "Requests, reviews and merges you watch appear here.") : h("div", { class: "list-card" }, state.db.activity.map((x) => h("div", { class: "list-item" }, avatar(personName(x.actorId), isAgent(x.actorId) || x.byAgent), h("div", { class: "grow" }, h("strong", null, personName(x.actorId)), ` ${x.summary}`, h("div", { class: "muted small" }, `${project(state.db, x.projectId).name} · ${when(x.at)}`)))));
    return h(
      "div",
      { class: "page" },
      viewHeader("Activity", `${unread()} unread`, ui.tab === "notifications" ? button("Mark all read", { size: "sm", disabled: !unread(), onClick: () => {
        state.db.notifications.forEach((n) => n.read = !0), render();
      } }) : null),
      tabs(ui.tab, [["notifications", "Notifications", unread()], ["activity", "All activity"]], (v) => {
        ui.tab = v, render();
      }, "Activity"),
      h("div", { class: "tab-body" }, body)
    );
  }

  // client/src/bridge.ts
  function bridgeInvoke() {
    let internals = window.__TAURI_INTERNALS__;
    return internals && typeof internals.invoke == "function" ? internals.invoke : null;
  }
  function inDesktop() {
    return bridgeInvoke() !== null;
  }
  function $(id) {
    let el = document.getElementById(id);
    if (!el) throw new Error(`launcher element #${id} is missing`);
    return el;
  }
  function show(el, on) {
    el.hidden = !on;
  }
  function say(id, text) {
    let el = $(id);
    el.textContent = text || "", el.getAttribute("role") || show(el, !!text);
  }
  function message(e) {
    return typeof e == "string" ? e : (e instanceof Error ? e.message : e && typeof e == "object" && "message" in e ? String(e.message) : "") || "Something went wrong. Please try again.";
  }
  function busy(button2, on, working2) {
    let b = button2;
    on ? (b.dataset.label = b.textContent ?? "", b.textContent = working2 ?? "") : b.dataset.label && (b.textContent = b.dataset.label), b.disabled = on;
  }
  var invoke = null, current = null;
  function desktopState() {
    return current;
  }
  async function connect(address, errorId, button2, working2) {
    if (!invoke) return !1;
    say(errorId, ""), busy(button2, !0, working2);
    try {
      let opened = await invoke("set_server", { address });
      say("current", "Opening " + opened + " ...");
    } catch (e) {
      return say(errorId, message(e)), busy(button2, !1), !1;
    }
    return !0;
  }
  async function openServer() {
    if (!invoke || !current || !current.server) return null;
    try {
      return await invoke("set_server", { address: current.server });
    } catch (e) {
      return Promise.reject(new Error(message(e)));
    }
  }
  async function loadKey() {
    if (!invoke) return;
    let status = await invoke("device_key_status");
    if (show($("key-public"), !1), !status.exists) {
      show($("key-none"), !0), show($("key-create"), !0), show($("key-have"), !1);
      return;
    }
    if (show($("key-none"), !1), show($("key-create"), !1), status.damaged) {
      show($("key-have"), !1), say("key-error", status.recovery);
      return;
    }
    show($("key-have"), !0), $("key-where").textContent = "Stored at " + status.path;
  }
  function wire() {
    $("use-cloud").addEventListener("click", function() {
      current && connect(current.cloud, "cloud-error", $("use-cloud"), "Opening ...");
    }), $("confirm").addEventListener("click", function() {
      !current || !current.unconfirmed || connect(current.unconfirmed, "confirm-error", $("confirm"), "Opening ...");
    }), $("own-form").addEventListener("submit", async function(event) {
      event.preventDefault(), await connect($("address").value, "address-error", $("connect"), "Connecting ...") || $("address").focus();
    }), $("signin-form").addEventListener("submit", async function(event) {
      if (event.preventDefault(), !!invoke) {
        say("link-error", ""), busy($("signin-go"), !0, "Signing in ...");
        try {
          await invoke("open_sign_in_link", { link: $("link").value }), $("link").value = "";
        } catch (e) {
          say("link-error", message(e)), busy($("signin-go"), !1), $("link").focus();
          return;
        }
        busy($("signin-go"), !1);
      }
    }), $("browser-signin-go").addEventListener("click", async function() {
      if (invoke) {
        say("browser-signin-error", ""), say("browser-signin-status", ""), show($("browser-signin-pairing"), !1), busy($("browser-signin-go"), !0, "Opening your browser ...");
        try {
          let pairing = await invoke("start_browser_sign_in");
          $("browser-signin-code").textContent = pairing, show($("browser-signin-pairing"), !0), say(
            "browser-signin-status",
            "Your browser has opened. Sign in there, check that the pairing code matches, choose Continue, and you will come back to this app. This request is good for 10 minutes."
          );
        } catch (e) {
          say("browser-signin-error", message(e));
        }
        busy($("browser-signin-go"), !1);
      }
    }), $("key-create").addEventListener("click", async function() {
      if (invoke) {
        say("key-error", ""), say("key-status", ""), busy($("key-create"), !0, "Creating ...");
        try {
          let publicKey = await invoke("device_key_create");
          await loadKey(), $("public-key").value = publicKey, show($("key-show"), !1), show($("key-public"), !0), say("key-status", "Key created. Add its public key in the web app to use it.");
        } catch (e) {
          say("key-error", message(e));
          try {
            await loadKey();
          } catch {
          }
        }
        busy($("key-create"), !1);
      }
    }), $("key-show").addEventListener("click", async function() {
      if (invoke) {
        say("key-error", ""), say("key-status", ""), busy($("key-show"), !0, "Reading the key ...");
        try {
          $("public-key").value = await invoke("device_public_key"), show($("key-show"), !1), show($("key-public"), !0);
        } catch (e) {
          say("key-error", message(e));
        }
        busy($("key-show"), !1);
      }
    }), $("key-copy").addEventListener("click", async function() {
      let field2 = $("public-key");
      try {
        await navigator.clipboard.writeText(field2.value), say("key-status", "Public key copied.");
      } catch {
        field2.focus(), field2.select(), say("key-status", "Select the public key and copy it with your keyboard.");
      }
    });
  }
  async function startBridge() {
    if (invoke = bridgeInvoke(), !invoke || !document.getElementById("launcher")) return null;
    wire();
    try {
      current = await invoke("get_state");
    } catch (e) {
      return say("app-error", message(e)), null;
    }
    let state2 = current, phone = state2.platform === "android" || state2.platform === "ios";
    if ($("about").textContent = (phone ? "Dynodoc " : "Dynodoc desktop ") + state2.version + " (" + state2.platform + ")", phone && ($("lede").textContent = "Dynodoc opens your workspace in this app. Choose the Dynodoc cloud, or the address of a server your organisation runs. To choose a different server later, sign out in Dynodoc: this screen comes back."), $("help").href = state2.support, state2.unconfirmed && ($("unconfirmed-address").textContent = state2.unconfirmed, show($("unconfirmed"), !0)), state2.server && (say("current", "Currently set to " + state2.server + "."), $("address").value = state2.server === state2.cloud ? "" : state2.server, show($("browser-signin"), state2.browser_signin), show($("signin-if-button"), state2.browser_signin), show($("signin"), !0)), state2.tools) {
      show($("key"), !0);
      try {
        await loadKey();
      } catch (e) {
        say("key-error", message(e));
      }
      try {
        let tools = await invoke("tools_version");
        $("about").textContent += ". Tools: " + tools + ".";
      } catch (e) {
        $("about").textContent += ". Local tools unavailable: " + message(e);
      }
    }
    return state2;
  }
  function launcherSection() {
    return location.hash === "#signin" ? "signin" : location.hash === "#key" ? "key" : null;
  }
  function focusSection(section2) {
    section2 === "signin" && !$("browser-signin").hidden && $("browser-signin-go").focus(), section2 === "key" && !$("key").hidden && $("key").scrollIntoView();
  }

  // client/src/views/shell.ts
  var NAV = [
    {
      group: "Version control",
      items: [
        { view: "changes", label: "Changes", icon: "changes", count: () => working(state.db, state.ui.projectId, currentBranch()).length, shortcut: "Ctrl+1" },
        { view: "history", label: "History", icon: "history", shortcut: "Ctrl+2" },
        { view: "requests", label: "Change requests", icon: "request", count: () => state.db.requests.filter((r) => r.projectId === state.ui.projectId && r.status === "open").length, shortcut: "Ctrl+3" }
      ]
    },
    {
      group: "Library",
      items: [
        { view: "documents", label: "Documents", icon: "documents", shortcut: "Ctrl+4" },
        { view: "projects", label: "Projects", icon: "projects" },
        { view: "connectors", label: "Connectors", icon: "connectors" }
      ]
    },
    {
      group: "Team",
      items: [
        { view: "activity", label: "Activity", icon: "activity", count: unread, shortcut: "Ctrl+6" },
        { view: "agent", label: "Agent", icon: "agent", shortcut: "Ctrl+5" }
      ]
    }
  ], VIEW_LABEL = {
    changes: "Changes",
    history: "History",
    requests: "Change requests",
    documents: "Documents",
    projects: "Projects",
    connectors: "Connectors",
    activity: "Activity",
    agent: "Agent",
    settings: "Settings"
  };
  function navRow(it) {
    let n = it.count?.() ?? 0, on = state.ui.view === it.view;
    return h(
      "button",
      { type: "button", class: cx("nav-row", on && "is-selected"), "aria-current": on ? "page" : void 0, "data-view": it.view, title: it.shortcut ? `${it.label} (${it.shortcut})` : it.label, onClick: () => go(it.view) },
      icon(it.icon, 16),
      h("span", { class: "nav-label" }, it.label),
      n ? h("span", { class: "nav-count" }, String(n)) : null
    );
  }
  function picker(label, value, iconName, onClick, id, title) {
    return h(
      "button",
      { type: "button", class: "picker-row", id, title, "aria-haspopup": "menu", onClick },
      icon(iconName, 16),
      h("span", { class: "picker-text" }, h("span", { class: "picker-label" }, label), h("span", { class: "picker-value" }, value)),
      icon("chevron", 14, "icon picker-chev")
    );
  }
  function sidebar() {
    let ctx = branchContext(), p2 = currentProject(), open2 = state.db.requests.filter((r) => r.projectId === p2.id && r.status === "open").sort((a, b) => b.createdAt - a.createdAt), ds = desktopState();
    return h(
      "aside",
      { class: "sidebar", "aria-label": "Navigation" },
      h("div", { class: "brand-row" }, h("button", { type: "button", class: "icon-btn", "aria-label": "Hide sidebar", title: "Hide sidebar (Ctrl+\\)", onClick: () => {
        state.prefs.sidebarOpen = !1, render();
      } }, icon("sidebar", 16)), h("img", { src: "mark.svg", alt: "", class: "mark-img", width: 18, height: 18 }), h("span", { class: "brand-name" }, "Dynodoc")),
      h(
        "div",
        { class: "pickers" },
        picker("Current project", p2.name, "repo", (e) => openProjectList(e.currentTarget), "project-picker", "Projects (Ctrl+T)"),
        h(
          "div",
          { class: "branch-picker" },
          picker("Current branch", branchLabel(), "branch", (e) => openBranchList(e.currentTarget), "branch-picker", "Branches (Ctrl+B)"),
          h("button", { type: "button", class: "icon-btn branch-menu-btn", "aria-label": "Branch menu", title: "Branch menu", "aria-haspopup": "menu", onClick: (e) => openBranchMenu(e.currentTarget) }, icon("more", 16))
        ),
        ctx.kind === "draft" ? h("div", { class: "branch-state" }, ctx.behind ?? 0 ? h("span", { title: "Revisions of main this draft does not have" }, icon("down", 12), `${ctx.behind} behind`) : null, ctx.ahead ?? 0 ? h("span", { title: "Commits not pushed" }, icon("up", 12), `${ctx.ahead} to push`) : null, h("span", { class: "muted" }, ctx.state === "in_review" ? "in review" : ctx.state)) : null
      ),
      h("button", { type: "button", class: "search-row", onClick: () => openPalette() }, icon("search", 15), h("span", null, "Search"), h("span", { class: "spacer" }), kbd("Ctrl+K")),
      h(
        "nav",
        { class: "nav", "data-scroll-key": "sidebar" },
        NAV.map((g) => h("div", { class: "nav-group" }, h("div", { class: "nav-group-label" }, g.group), g.items.map(navRow))),
        open2.length ? h("div", { class: "nav-group" }, h("div", { class: "nav-group-label" }, "Open requests"), open2.map((r) => {
          let st = reviewState(state.db, r);
          return h("button", { type: "button", class: cx("thread-row", state.ui.view === "requests" && state.ui.requests.selected === r.id && "is-selected"), onClick: () => {
            state.ui.requests.selected = r.id, state.ui.requests.filter = "open", go("requests");
          } }, h("span", { class: cx("thread-dot", st.satisfied ? "is-ready" : r.conflicts.length ? "is-conflict" : "is-waiting") }), h("span", { class: "thread-title" }, `#${r.number} ${r.name}`), h("span", { class: "muted small" }, short(r.createdAt)));
        })) : null
      ),
      h(
        "div",
        { class: "sidebar-foot" },
        h("button", { type: "button", class: cx("icon-btn", state.ui.view === "settings" && "is-active"), "aria-label": "Settings", title: "Settings (Ctrl+,)", "data-view": "settings", onClick: () => go("settings") }, icon("settings", 16)),
        h("button", { type: "button", class: "icon-btn", "aria-label": "Toggle light and dark", title: "Light or dark (Ctrl+Shift+L)", onClick: () => {
          toggleTheme(), render();
        } }, icon(scheme() === "dark" ? "sun" : "moon", 16)),
        h("button", { type: "button", class: "icon-btn", "aria-label": "Account and server", title: ds?.server ? `Server: ${ds.server}` : "Account and server", id: "nav-account", onClick: () => {
          state.ui.settings.section = "accounts", go("settings");
        } }, icon("user", 16)),
        h("span", { class: "spacer" }),
        h("button", { type: "button", class: "release-pill", title: "About this release", onClick: () => {
          state.ui.settings.section = "about", go("settings");
        } }, "Pre-alpha release")
      )
    );
  }
  function crumbs() {
    let p2 = currentProject(), v = state.ui.view, parts = [];
    if (v !== "settings" && v !== "connectors" && v !== "projects" && v !== "activity" && parts.push(h("button", { type: "button", class: "crumb", onClick: (e) => openProjectList(e.currentTarget) }, p2.name), h("span", { class: "crumb-sep" }, "/")), parts.push(h("span", { class: "crumb crumb-current" }, VIEW_LABEL[v])), (v === "changes" || v === "history") && parts.push(h("span", { class: "crumb-sep" }, "/"), h("span", { class: "crumb-branch" }, icon("branch", 12), branchLabel())), v === "requests" && state.ui.requests.selected) {
      let r = state.db.requests.find((x) => x.id === state.ui.requests.selected);
      r && parts.push(h("span", { class: "crumb-sep" }, "/"), h("span", { class: "crumb" }, `#${r.number}`));
    }
    return parts;
  }
  function topbar() {
    let ctx = branchContext(), ahead = ctx.ahead ?? 0, ds = desktopState(), fetching = state.ui.fetching;
    return h(
      "header",
      { class: "topbar" },
      state.prefs.sidebarOpen ? null : h("button", { type: "button", class: "icon-btn", "aria-label": "Show sidebar", onClick: () => {
        state.prefs.sidebarOpen = !0, render();
      } }, icon("sidebar", 16)),
      h("nav", { class: "crumbs-bar", "aria-label": "Breadcrumb" }, crumbs()),
      h("span", { class: "spacer" }),
      h(
        "span",
        { class: "status-pills" },
        pill("Sample data", "warning", { mono: !1, title: "This release runs on sample data. See Settings, About." }),
        ds?.server ? h("button", { type: "button", class: "server-btn", title: "Open your workspace on this server in this window", onClick: () => {
          openServer().catch((e) => toast("error", e instanceof Error ? e.message : String(e)));
        } }, icon("globe", 13), new URL(ds.server).host, icon("external", 12)) : pill(ds ? "No server" : "Offline copy", "neutral", { mono: !1, title: ds ? "Choose a server in Settings, Accounts" : "This copy connects to nothing" }),
        ctx.kind === "draft" && (ctx.behind ?? 0) ? pill(`${ctx.behind} behind main`, "warning", { mono: !1 }) : null
      ),
      h(
        "button",
        { type: "button", class: cx("sync-btn", fetching && "is-busy"), id: "sync-button", title: ahead ? "Push (Ctrl+Shift+P)" : "Fetch (Ctrl+Shift+T)", disabled: fetching, onClick: () => ahead ? sync() : fetchNow() },
        icon(ahead ? "up" : "sync", 15, cx("icon", fetching && "spin")),
        h("span", { class: "sync-text" }, h("span", { class: "sync-title" }, fetching ? ahead ? "Pushing…" : "Fetching…" : ahead ? `Push ${ahead} commit${ahead === 1 ? "" : "s"}` : "Fetch"), h("span", { class: "sync-sub" }, `Last fetched ${when(state.db.lastFetched)}`))
      )
    );
  }
  function bottomBar() {
    return h(
      "nav",
      { class: "bottombar", "aria-label": "Navigation" },
      [["changes", "Changes", "changes"], ["history", "History", "history"], ["requests", "Requests", "request"], ["agent", "Agent", "agent"]].map(([v, l, i]) => h("button", { type: "button", class: cx("bottom-item", state.ui.view === v && "is-on"), onClick: () => go(v) }, icon(i, 18), h("span", null, l))),
      h("button", { type: "button", class: cx("bottom-item", !["changes", "history", "requests", "agent"].includes(state.ui.view) && "is-on"), onClick: (e) => openMenu(e.currentTarget, [
        { label: "Documents", icon: "documents", onSelect: () => go("documents") },
        { label: "Projects", icon: "projects", onSelect: () => go("projects") },
        { label: "Connectors", icon: "connectors", onSelect: () => go("connectors") },
        { label: "Activity", icon: "activity", onSelect: () => go("activity") },
        { label: "Settings", icon: "settings", onSelect: () => go("settings") },
        { label: "", separator: !0 },
        { label: "Switch project…", icon: "repo", onSelect: () => openProjectList(document.getElementById("topbar-anchor") ?? document.body) },
        { label: "Switch branch…", icon: "branch", onSelect: () => openBranchList(document.getElementById("topbar-anchor") ?? document.body) }
      ], { align: "right", width: 240 }) }, icon("more", 18), h("span", null, "More"))
    );
  }
  function shell(main) {
    return h(
      "div",
      { class: cx("app", !state.prefs.sidebarOpen && "sidebar-closed") },
      state.prefs.sidebarOpen ? sidebar() : null,
      h("div", { class: "main-col" }, topbar(), h("span", { id: "topbar-anchor", class: "anchor" }), h("main", { class: "content", id: "content" }, main)),
      bottomBar()
    );
  }

  // client/src/data/connectors.ts
  var ADDIN_CAVEAT = "Not yet tried in an installed editor. No store listing exists.", CONNECTORS = [
    { id: "word", host: "word", name: "Word add-in", category: "Editor add-in", summary: "Captures paragraph edits in Word and pushes them for review; previews team changes in the document.", auth: "Pairing code, then a connection key for one file (7 days)", hosts: ["Your Dynodoc server"], status: "Built", caveat: ADDIN_CAVEAT, where: "extensions/word (Office manifest)" },
    { id: "excel", host: "excel", name: "Excel add-in", category: "Editor add-in", summary: "Captures cell values and formulas; pushes a change bundle; previews the team version.", auth: "Pairing code, then a connection key for one file (7 days)", hosts: ["Your Dynodoc server"], status: "Built", caveat: ADDIN_CAVEAT, where: "extensions/excel (Office manifest)" },
    { id: "powerpoint", host: "powerpoint", name: "PowerPoint add-in", category: "Editor add-in", summary: "Captures slide text and notes; pushes changes for review.", auth: "Pairing code, then a connection key for one file (7 days)", hosts: ["Your Dynodoc server"], status: "Built", caveat: ADDIN_CAVEAT, where: "extensions/powerpoint (Office manifest)" },
    { id: "google-docs", host: "google-docs", name: "Google Docs add-on", category: "Editor add-in", summary: "A sidebar in Google Docs that captures paragraph edits and pushes them for review.", auth: "Pairing code, then a connection key for one file (7 days)", hosts: ["Your Dynodoc server"], status: "Built", caveat: ADDIN_CAVEAT, where: "extensions/google-docs (Apps Script)" },
    { id: "google-sheets", host: "google-sheets", name: "Google Sheets add-on", category: "Editor add-in", summary: "A sidebar in Google Sheets that captures cell edits.", auth: "Pairing code, then a connection key for one file (7 days)", hosts: ["Your Dynodoc server"], status: "Built", caveat: ADDIN_CAVEAT, where: "extensions/google-sheets (Apps Script)" },
    { id: "google-slides", host: "google-slides", name: "Google Slides add-on", category: "Editor add-in", summary: "A sidebar in Google Slides that captures slide text.", auth: "Pairing code, then a connection key for one file (7 days)", hosts: ["Your Dynodoc server"], status: "Built", caveat: ADDIN_CAVEAT, where: "extensions/google-slides (Apps Script)" },
    { id: "kobo", name: "KoboToolbox", category: "Import", summary: "Imports a form or its current responses as a spreadsheet.", auth: "API token (used once, not saved)", hosts: ["kf.kobotoolbox.org", "your own server"], status: "Built", caveat: "Runs on the server; the token is used for one import and not saved.", where: "web workspace, Import from research tool" },
    { id: "odk", name: "ODK Central", category: "Import", summary: "Imports a form or its submissions as a spreadsheet.", auth: "Web User session token", hosts: ["your-project.getodk.cloud", "your own server"], status: "Built", caveat: "App User tokens do not give this access.", where: "web workspace, Import from research tool" },
    { id: "redcap", name: "REDCap", category: "Import", summary: "Imports a project’s instruments or records.", auth: "API token", hosts: ["your institution's REDCap"], status: "Built", caveat: "The token is used for one import and not saved.", where: "web workspace, Import from research tool" },
    { id: "surveycto", name: "SurveyCTO", category: "Import", summary: "Imports a form definition or its data.", auth: "API user and password", hosts: ["your-server.surveycto.com"], status: "Built", caveat: "Used for one import and not saved.", where: "web workspace, Import from research tool" },
    { id: "surveymonkey", name: "SurveyMonkey", category: "Import", summary: "Imports a survey or its responses.", auth: "API access token", hosts: ["api.surveymonkey.com"], status: "Built", caveat: "Used for one import and not saved.", where: "web workspace, Import from research tool" },
    { id: "gforms", name: "Google Forms", category: "Import", summary: "Imports a form as a questionnaire.", auth: "Google sign-in (when the server is configured for it)", hosts: ["Google"], status: "Built", caveat: "Available only on servers with Google sign-in configured.", where: "web workspace, Import from Google Forms" },
    { id: "cli", name: "engine-cli sync", category: "History", summary: "Publishes signed commits of a project to its shared history; verifies receipts.", auth: "Device signing key (this computer’s key)", hosts: ["Your Dynodoc server"], status: "Built", caveat: "Command line; the desktop app bundles engine-cli for its key only.", where: "crates/engine-cli" },
    { id: "agent-api", name: "Agent API", category: "Agent access", summary: "Lets an AI agent read files, outlines and diffs and propose changes under the project’s agent policy.", auth: "Agent key enrolled under Profile, Signing keys", hosts: ["Your Dynodoc server"], status: "Built", caveat: "Proposals wait for a person; an agent cannot merge.", where: "/projects/{id}/agent/*" }
  ], HOST_LABEL = {
    word: "Microsoft Word",
    excel: "Microsoft Excel",
    powerpoint: "Microsoft PowerPoint",
    "google-docs": "Google Docs",
    "google-sheets": "Google Sheets",
    "google-slides": "Google Slides"
  };

  // client/src/ui/diffView.ts
  function segs(list2, word) {
    return list2.length ? list2.map((g) => g.t === "same" || !word ? h("span", null, g.s) : h("span", { class: g.t === "add" ? "w-add" : "w-del" }, g.s)) : [h("span", { class: "seg-empty" }, "(empty)")];
  }
  function cols(kinds) {
    return h("colgroup", null, kinds.map((k) => h("col", { class: `col-${k}` })));
  }
  function diffView(base, edits2, o = {}) {
    let p2 = state.prefs, layout = o.layout ?? p2.diffLayout, context = o.context ?? p2.diffContext, list2 = hunks(base, edits2, context);
    if (!list2.length) return h("div", { class: "diff-empty" }, o.emptyText ?? "No changes.");
    let excluded = new Set(o.excluded ?? []), seenToggle = /* @__PURE__ */ new Set(), control = (blockId, changed) => {
      if (!changed) return h("span", { class: "diff-gutter-ctl" });
      let parts = [];
      return o.onToggle && !seenToggle.has(blockId) ? (seenToggle.add(blockId), parts.push(
        h("input", {
          type: "checkbox",
          class: "check",
          checked: !excluded.has(blockId),
          "aria-label": `Include ${blockId}`,
          onChange: (e) => o.onToggle?.(blockId, e.target.checked)
        })
      )) : p2.checkMarks && !o.onToggle && parts.push(h("span", { class: "diff-mark" }, icon("check", 12))), h("span", { class: "diff-gutter-ctl" }, parts);
    }, after = (blockId, changed) => {
      let out = [];
      return changed && o.onComment && out.push(h("button", { type: "button", class: "diff-comment-btn", title: "Comment on this change", "aria-label": "Comment on this change", onClick: () => o.onComment?.(blockId) }, icon("comment", 13))), out;
    }, commentRow = (blockId, span) => {
      let c = o.comments?.[blockId];
      return !c || !c.length ? null : h("tr", { class: "diff-comments" }, h("td", { colspan: span }, h("div", { class: "diff-comment-list" }, c)));
    };
    return h(
      "div",
      { class: cx("diff", `diff-${layout}`), css: { "--tab-size": String(p2.tabSize) } },
      list2.map((hk) => {
        if (layout === "split") {
          let lines2 = split(hk), done = /* @__PURE__ */ new Set();
          return h(
            "table",
            { class: "diff-table" },
            cols(["ctl", "no", "cell", "no", "cell", "after"]),
            h("thead", null, h("tr", null, h("th", { colspan: 6, class: "diff-hunk" }, hk.header))),
            h(
              "tbody",
              null,
              lines2.map((l) => {
                let changed = l.kind !== "context", first = !done.has(l.blockId);
                return done.add(l.blockId), [
                  h(
                    "tr",
                    { class: cx("diff-row", `k-${l.kind}`, excluded.has(l.blockId) && "is-excluded") },
                    h("td", { class: "diff-ctl" }, first ? control(l.blockId, changed) : null),
                    h("td", { class: "diff-no" }, l.left ? String(l.left.no) : ""),
                    h("td", { class: cx("diff-cell", l.left && changed && (l.kind === "del" || l.kind === "edit") && "side-del", !l.left && "side-none") }, l.left ? [h("div", { class: "diff-label" }, l.left.label), h("div", { class: "diff-text" }, segs(l.left.segs, p2.wordDiff))] : null),
                    h("td", { class: "diff-no" }, l.right ? String(l.right.no) : ""),
                    h("td", { class: cx("diff-cell", l.right && changed && (l.kind === "add" || l.kind === "edit") && "side-add", !l.right && "side-none") }, l.right ? [h("div", { class: "diff-label" }, l.right.label), h("div", { class: "diff-text" }, segs(l.right.segs, p2.wordDiff))] : null),
                    h("td", { class: "diff-after" }, after(l.blockId, changed))
                  ),
                  commentRow(l.blockId, 6)
                ];
              })
            )
          );
        }
        let lines = unified(hk);
        return h(
          "table",
          { class: "diff-table" },
          cols(["ctl", "no", "no", "sign", "cell", "after"]),
          h("thead", null, h("tr", null, h("th", { colspan: 6, class: "diff-hunk" }, hk.header))),
          h(
            "tbody",
            null,
            lines.map((l, i) => {
              let changed = l.sign !== " ", last = !lines[i + 1] || lines[i + 1].blockId !== l.blockId;
              return [
                h(
                  "tr",
                  { class: cx("diff-row", l.sign === "-" ? "k-del" : l.sign === "+" ? "k-add" : "k-context", excluded.has(l.blockId) && "is-excluded") },
                  h("td", { class: "diff-ctl" }, control(l.blockId, changed)),
                  h("td", { class: "diff-no" }, l.oldNo === void 0 ? "" : String(l.oldNo)),
                  h("td", { class: "diff-no" }, l.newNo === void 0 ? "" : String(l.newNo)),
                  h("td", { class: "diff-sign", "aria-hidden": "true" }, l.sign),
                  h("td", { class: "diff-cell" }, h("div", { class: "diff-label" }, l.label), h("div", { class: "diff-text" }, segs(l.segs, p2.wordDiff))),
                  h("td", { class: "diff-after" }, l.sign === "+" || l.sign === "-" && last ? after(l.blockId, changed) : null)
                ),
                last ? commentRow(l.blockId, 6) : null
              ];
            })
          )
        );
      })
    );
  }

  // client/src/views/documents.ts
  var HOSTS_FOR = {
    document: ["word", "google-docs"],
    spreadsheet: ["excel", "google-sheets"],
    presentation: ["powerpoint", "google-slides"]
  };
  function activeKeys(d) {
    return d.connections.filter((k) => !k.revoked && k.expiresAt > Date.now());
  }
  function openInEditor(docId) {
    let d = doc(state.db, docId), pref = state.prefs.editorFor[d.kind], host = HOSTS_FOR[d.kind][pref === "office" ? 0 : 1], keys2 = activeKeys(d).filter((k) => k.host === host);
    openDialog({
      title: `Open in ${HOST_LABEL[host]}`,
      width: 560,
      body: () => [
        h("p", { class: "dialog-text" }, `Dynodoc edits happen in ${HOST_LABEL[host]} with the Dynodoc add-in, not in this window. Open ${d.filename} there; the add-in captures your edits and they appear under Changes.`),
        h(
          "dl",
          { class: "facts" },
          h("dt", null, "Add-in"),
          h("dd", null, CONNECTORS.find((c) => c.host === host)?.name ?? host),
          h("dt", null, "Connection"),
          h("dd", null, keys2.length ? `Connected with a key that expires ${when(keys2[0].expiresAt)}${keys2[0].lastUsed ? `, last used ${when(keys2[0].lastUsed)}` : ""}` : "Not connected: create a connection key under Sync & connections."),
          h("dt", null, "Install"),
          h("dd", null, "Settings, Integrations has the steps for each editor.")
        ),
        notice("info", "This release does not start the editor for you. Your preferred editor per file type is in Settings, Integrations.")
      ],
      actions: () => [button("Close", { onClick: closeDialog }), button("Sync & connections", { variant: "primary", onClick: () => {
        closeDialog(), state.ui.view = "documents", state.ui.documents.selected = d.id, state.ui.documents.tab = "sync", render();
      } })]
    });
  }
  function fileTable(docs) {
    return h(
      "table",
      { class: "table" },
      h("thead", null, h("tr", null, h("th", null, "Name"), h("th", null, "Latest"), h("th", null, "Last change"), h("th", null, "Drafts"), h("th", null, "Requests"), h("th", null, "Editor"), h("th", null, "Updated"))),
      h(
        "tbody",
        null,
        docs.map((d) => {
          let last = branchCommits(state.db, d.id, "main").slice(-1)[0], drafts2 = state.db.drafts.filter((x) => x.docId === d.id && (x.state === "private" || x.state === "in_review")).length, reqs = state.db.requests.filter((r) => r.docId === d.id && r.status === "open").length, keys2 = activeKeys(d);
          return h(
            "tr",
            { class: cx("row-link", state.ui.documents.selected === d.id && "is-selected"), tabindex: "0", onClick: () => {
              state.ui.documents.selected = d.id, render();
            }, onKeydown: (e) => {
              e.key === "Enter" && (state.ui.documents.selected = d.id, render());
            } },
            h("td", null, h("span", { class: "cell-file" }, fileIcon(d.kind), h("span", null, h("strong", null, d.title), h("span", { class: "muted small block" }, d.filename)))),
            h("td", { class: "mono" }, `r${mainSeq(state.db, d.id)}`),
            h("td", null, last ? h("span", null, last.summary, h("span", { class: "muted" }, ` · ${personName(last.authorId)}`)) : "—"),
            h("td", null, drafts2 ? String(drafts2) : h("span", { class: "muted" }, "0")),
            h("td", null, reqs ? pill(`${reqs} open`, "success") : h("span", { class: "muted" }, "0")),
            h("td", null, keys2.length ? pill(`${HOST_LABEL[keys2[0].host]}`, "success", { mono: !1 }) : pill("not connected", "neutral")),
            h("td", { class: "muted" }, last ? short(last.at) : "")
          );
        })
      )
    );
  }
  function overview(d) {
    let commits = branchCommits(state.db, d.id, "main"), last = commits[commits.length - 1];
    return h(
      "div",
      { class: "doc-overview" },
      h(
        "div",
        { class: "doc-facts card" },
        h(
          "dl",
          { class: "facts" },
          h("dt", null, "Type"),
          h("dd", null, KIND_LABEL[d.kind]),
          h("dt", null, "Team version"),
          h("dd", null, `main, revision ${mainSeq(state.db, d.id)}`),
          h("dt", null, "Revision hash"),
          h("dd", null, last ? hash(last.chainHash, 16) : "—"),
          h("dt", null, "Last change"),
          h("dd", null, last ? `${last.summary}, ${personName(last.authorId)}, ${when(last.at)}` : "—"),
          h("dt", null, "Created"),
          h("dd", null, when(d.createdAt))
        ),
        h(
          "div",
          { class: "row-actions" },
          button("Verify hash chain", { icon: "shield", size: "sm", onClick: () => {
            let r = runVerify(d.id);
            act(() => r.bad ? `The hash chain does not verify at ${r.bad} revisions` : `Verified: the hash chain is intact through r${mainSeq(state.db, d.id)}`);
          } }),
          button(`Open in ${HOST_LABEL[HOSTS_FOR[d.kind][state.prefs.editorFor[d.kind] === "office" ? 0 : 1]]}`, { icon: "external", size: "sm", onClick: () => openInEditor(d.id) }),
          button("New draft…", { icon: "branch", size: "sm", onClick: () => newDraftDialog(d.id) }),
          button("Download", { icon: "down", size: "sm", disabled: !0, title: "Exporting a native file is in the web workspace; not in this release of the client." })
        )
      ),
      h("div", { class: "section-title" }, "Content at main", h("span", { class: "muted small" }, " · read only")),
      h(
        "div",
        { class: cx("doc-preview", `preview-${d.kind}`) },
        d.blocks.map(
          (b) => h("div", { class: cx("pv-block", /Heading|Title/.test(b.label) && "is-heading") }, h("span", { class: "pv-label" }, b.label), h("span", { class: "pv-text" }, b.text))
        )
      )
    );
  }
  function versions(d) {
    return h(
      "div",
      null,
      h("div", { class: "row-actions" }, button("Save named version…", { icon: "plus", size: "sm", onClick: () => saveVersionDialog(d) })),
      d.versions.length ? h(
        "div",
        { class: "list-card" },
        d.versions.map(
          (v) => h(
            "div",
            { class: "list-item" },
            icon("pin", 15),
            h("div", { class: "grow" }, h("div", null, h("strong", null, v.name), h("span", { class: "muted" }, ` · through r${v.throughSeq} · ${personName(v.authorId)} · ${when(v.at)}`)), v.note ? h("div", { class: "muted small" }, v.note) : null),
            button("Compare with main", { size: "sm", onClick: () => compareVersion(d, v.id) }),
            button("Restore as new changes", { size: "sm", onClick: () => act(() => {
              let dr = restoreVersion(state.db, d.id, v.id);
              return switchBranch(dr.id), `Restored "${v.name}" into the draft ${dr.name}`;
            }) })
          )
        )
      ) : empty("pin", "No named versions", "Name a version to find it again: an edition, a submitted copy.")
    );
  }
  function compareVersion(d, versionId) {
    let v = d.versions.find((x) => x.id === versionId);
    if (!v) return;
    let then = blocksAt(state.db, d.id, v.throughSeq), edits2 = compareBlocks(then, d.blocks);
    openDialog({ title: `"${v.name}" compared with main`, width: 900, body: () => diffView(then, edits2, { emptyText: "main still matches this version." }), actions: () => button("Close", { onClick: closeDialog }) });
  }
  function saveVersionDialog(d) {
    let name2 = "", note = "";
    openDialog({
      title: "Save named version",
      width: 520,
      body: () => [
        h("p", { class: "dialog-text" }, `Names main as it is now (r${mainSeq(state.db, d.id)}). History is not changed.`),
        field("Name", input(name2, (v) => {
          name2 = v;
        }, { id: "ver-name", autofocus: !0, placeholder: "Winter 2027 edition" })),
        field("Note", textarea(note, (v) => {
          note = v;
        }, { id: "ver-note", rows: 2 }))
      ],
      actions: () => [button("Cancel", { onClick: closeDialog }), button("Save version", { variant: "primary", onClick: () => {
        act(() => saveVersion(state.db, d.id, name2, note), "Saved the version") && closeDialog();
      } })]
    });
  }
  function drafts(d) {
    let list2 = state.db.drafts.filter((x) => x.docId === d.id).sort((a, b) => b.createdAt - a.createdAt);
    return h(
      "div",
      null,
      h("div", { class: "row-actions" }, button("New draft…", { icon: "plus", size: "sm", onClick: () => newDraftDialog(d.id) })),
      h(
        "div",
        { class: "list-card" },
        list2.map(
          (x) => h(
            "div",
            { class: "list-item" },
            icon("branch", 15),
            h("div", { class: "grow" }, h("div", null, h("strong", null, x.name), " ", pill(draftWord(x.state), draftTone(x.state))), h("div", { class: "muted small" }, `${personName(x.authorId)} · based on r${x.baseSeq} · ${branchCommits(state.db, d.id, x.id).length} commit(s) · ${when(x.createdAt)}${behind(state.db, x) && (x.state === "private" || x.state === "in_review") ? ` · ${behind(state.db, x)} behind main` : ""}`)),
            (x.state === "private" || x.state === "in_review") && x.authorId === state.db.me ? button("Switch to", { size: "sm", onClick: () => switchBranch(x.id) }) : null
          )
        ),
        list2.length ? null : h("div", { class: "muted small pad" }, "No drafts of this file.")
      )
    );
  }
  function discussions(d) {
    return h(
      "div",
      { class: "list-card" },
      d.discussions.map((t) => {
        let block = d.blocks.find((b) => b.id === t.blockId);
        return h(
          "div",
          { class: cx("thread", t.resolved && "is-resolved") },
          h("div", { class: "thread-quote" }, h("span", { class: "muted small" }, block?.label ?? t.blockId), h("div", { class: "quote" }, block?.text ?? "")),
          [{ authorId: t.authorId, text: t.text, at: t.at }, ...t.replies].map((c) => h("div", { class: "comment" }, avatar(personName(c.authorId)), h("div", { class: "comment-body" }, h("div", { class: "comment-meta" }, h("strong", null, personName(c.authorId)), h("span", { class: "muted" }, when(c.at))), h("div", { class: "comment-text" }, c.text)))),
          h(
            "div",
            { class: "row-actions" },
            button(t.resolved ? "Reopen" : "Resolve", { size: "sm", onClick: () => {
              t.resolved = !t.resolved, render();
            } }),
            button("Reply…", { size: "sm", variant: "ghost", onClick: () => replyDialog(t) })
          )
        );
      }),
      d.discussions.length ? null : h("div", { class: "muted small pad" }, "No discussions. Comments are added in the editor.")
    );
  }
  function replyDialog(t) {
    let text = "";
    openDialog({ title: "Reply", width: 480, body: () => textarea(text, (v) => {
      text = v;
    }, { id: "reply-text", rows: 3 }), actions: () => [button("Cancel", { onClick: closeDialog }), button("Reply", { variant: "primary", onClick: () => {
      text.trim() && (t.replies.push({ authorId: state.db.me, text: text.trim(), at: Date.now() }), closeDialog());
    } })] });
  }
  function people(d) {
    let p2 = project(state.db, d.projectId);
    return h(
      "div",
      null,
      h("div", { class: "card pad" }, h("div", { class: "srow-title" }, "Review rules"), h("div", { class: "muted small" }, `From the project ${p2.name}: ${p2.rules.protectMain ? "main is protected (edits go through review)" : "editors may write main"}, ${p2.rules.requiredApprovals} approval${p2.rules.requiredApprovals === 1 ? "" : "s"} needed, ${p2.rules.mergeRoles === "owners" ? "owners and managers" : "editors and above"} merge. Change them in Projects, Review rules.`)),
      h("div", { class: "list-card" }, p2.members.map((m) => h("div", { class: "list-item" }, avatar(personName(m.personId), isAgent(m.personId)), h("div", { class: "grow" }, h("strong", null, personName(m.personId)), h("div", { class: "muted small" }, person(state.db, m.personId).email)), badge(ROLE_LABEL[m.role]), h("span", { class: "muted small" }, m.via === "workspace" ? "via workspace" : "direct"))))
    );
  }
  function sync2(d) {
    let hosts = HOSTS_FOR[d.kind], host = hosts[0];
    return h(
      "div",
      null,
      h("div", { class: "section-title" }, "Connection keys"),
      h("p", { class: "muted small" }, "An add-in connects to one file with a key. A key works for 7 days, can read the file and its history and propose changes; it cannot merge."),
      h(
        "div",
        { class: "row-actions" },
        select(host, hosts.map((x) => [x, HOST_LABEL[x]]), (v) => {
          host = v;
        }, { label: "Editor" }),
        button("Create connection key", { size: "sm", variant: "primary", onClick: () => createKey(d, host) })
      ),
      h(
        "div",
        { class: "list-card" },
        d.connections.map((k) => {
          let expired = k.expiresAt <= Date.now();
          return h(
            "div",
            { class: "list-item" },
            icon("key", 15),
            h("div", { class: "grow" }, h("strong", null, HOST_LABEL[k.host]), h("div", { class: "muted small" }, `created ${when(k.createdAt)} · ${k.revoked ? "revoked" : expired ? "expired" : `expires ${when(k.expiresAt)}`}${k.lastUsed ? ` · last used ${when(k.lastUsed)}` : ""}`)),
            pill(k.revoked ? "revoked" : expired ? "expired" : "active", k.revoked || expired ? "neutral" : "success"),
            !k.revoked && !expired ? button("Revoke", { size: "sm", variant: "ghost", onClick: () => confirmDialog("Revoke connection key", h("p", null, `The ${HOST_LABEL[k.host]} add-in loses access to ${d.filename} at once.`), "Revoke", () => {
              k.revoked = !0, toast("success", "Key revoked"), render();
            }, { danger: !0 }) }) : null
          );
        }),
        d.connections.length ? null : h("div", { class: "muted small pad" }, "No connection keys.")
      ),
      h("div", { class: "section-title" }, "Read-only links"),
      h("div", { class: "row-actions" }, button("Publish a read-only copy", { size: "sm", onClick: () => {
        d.publicLinks.unshift({ id: `pl${Date.now()}`, createdAt: Date.now(), expiresAt: Date.now() + 30 * 864e5, revoked: !1 }), toast("success", "Published a read-only copy", "It shows main as it is now and does not include later edits."), render();
      } })),
      h("div", { class: "list-card" }, d.publicLinks.map((l) => h("div", { class: "list-item" }, icon("link", 15), h("div", { class: "grow" }, h("strong", null, "Published copy"), h("div", { class: "muted small" }, `${when(l.createdAt)} · ${l.revoked ? "revoked" : l.expiresAt ? `until ${when(l.expiresAt)}` : "until revoked"}`)), l.revoked ? pill("revoked") : button("Revoke", { size: "sm", variant: "ghost", onClick: () => {
        l.revoked = !0, render();
      } }))), d.publicLinks.length ? null : h("div", { class: "muted small pad" }, "No published copies.")),
      h("div", { class: "section-title" }, "Copies and similar files"),
      h("div", { class: "list-card" }, d.copies.map((c) => h("div", { class: "list-item" }, icon("documents", 15), h("div", { class: "grow" }, h("strong", null, c.title), h("div", { class: "muted small" }, `${c.relation.replace("_", " ")} · ${Math.round(c.score * 100)}% similar`)), pill(c.status), c.status === "open" ? [button("Invite to send changes", { size: "sm", onClick: () => {
        c.status = "invited", render();
      } }), button("Dismiss", { size: "sm", variant: "ghost", onClick: () => {
        c.status = "dismissed", render();
      } })] : null)), d.copies.length ? null : h("div", { class: "muted small pad" }, "Dynodoc has found no copies of this file."))
    );
  }
  function createKey(d, host) {
    let bytes = new Uint8Array(24);
    crypto.getRandomValues(bytes);
    let token = "ddk_sample_" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
    d.connections.unshift({ id: `ck${Date.now()}`, host, createdAt: Date.now(), expiresAt: Date.now() + 7 * 864e5, revoked: !1 }), openDialog({
      title: "Connection key created",
      width: 560,
      body: () => [
        h("p", { class: "dialog-text" }, `Paste this key into the ${HOST_LABEL[host]} add-in under "Advanced: connect with a key". It is shown once and works for 7 days.`),
        h("input", { class: "input mono", readonly: !0, value: token, id: "conn-key", "aria-label": "Connection key" }),
        notice("info", "Sample data: this key is random characters and connects nothing.")
      ],
      actions: () => [button("Copy", { onClick: async () => {
        try {
          await navigator.clipboard.writeText(token), toast("success", "Copied");
        } catch {
          toast("warning", "Select the key and copy it");
        }
      } }), button("Done", { variant: "primary", onClick: closeDialog })]
    });
  }
  function documentsView() {
    let ui = state.ui.documents, docs = docsOf(state.db, state.ui.projectId), sel = docs.find((d) => d.id === ui.selected);
    if (!sel)
      return h(
        "div",
        { class: "page" },
        viewHeader("Documents", `${docs.length} file${docs.length === 1 ? "" : "s"} in ${project(state.db, state.ui.projectId).name}`, button("New draft…", { icon: "branch", onClick: () => newDraftDialog() }), button("Add file", { icon: "plus", disabled: !0, title: "Uploading files is in the web workspace and the add-ins; not in this release of the client." })),
        docs.length ? h("div", { class: "card flush" }, fileTable(docs)) : empty("documents", "No files", "Add a file in the web workspace or from an add-in.")
      );
    let counts = { drafts: state.db.drafts.filter((x) => x.docId === sel.id).length, versions: sel.versions.length, disc: sel.discussions.filter((t) => !t.resolved).length }, body = ui.tab === "versions" ? versions(sel) : ui.tab === "drafts" ? drafts(sel) : ui.tab === "discussions" ? discussions(sel) : ui.tab === "people" ? people(sel) : ui.tab === "sync" ? sync2(sel) : overview(sel);
    return h(
      "div",
      { class: "page" },
      h("div", { class: "crumbs" }, h("button", { type: "button", class: "link", onClick: () => {
        ui.selected = null, render();
      } }, "Documents"), h("span", { class: "muted" }, " / "), sel.filename),
      viewHeader(sel.title, h("span", { class: "inline-meta" }, fileIcon(sel.kind, 14), `${sel.filename} · main r${mainSeq(state.db, sel.id)}`)),
      tabs(ui.tab, [["overview", "Overview"], ["versions", "Versions", counts.versions], ["drafts", "Drafts", counts.drafts], ["discussions", "Discussions", counts.disc], ["people", "People & rules"], ["sync", "Sync & connections"]], (v) => {
        ui.tab = v, render();
      }, "File sections"),
      h("div", { class: "tab-body" }, body)
    );
  }

  // client/src/views/changes.ts
  var OP = { edit: ["M", "Modified"], add: ["A", "Added"], remove: ["D", "Removed"] };
  function labelOf(w) {
    return w.edit.label ? w.edit.label : blocksOf(state.db, w.docId, w.branch).find((x) => x.id === w.edit.blockId)?.label ?? w.edit.blockId;
  }
  function commitStaged2() {
    let c = state.ui.changes, branch = currentBranch(), target = commitTarget(branchContext());
    if (!target.ok) return toast("warning", "Cannot commit here", target.note);
    if (!c.summary.trim()) {
      toast("warning", "A summary is required"), document.getElementById("commit-summary")?.focus();
      return;
    }
    act(() => {
      let out = commitStaged(state.db, state.ui.projectId, branch, c.summary, c.description);
      return c.summary = "", c.description = "", c.selected = null, `Committed ${out.length} file${out.length === 1 ? "" : "s"} to ${branchLabel(branch)}`;
    });
  }
  function discardOne(w) {
    let run = () => act(() => {
      discard(state.db, [w.id]);
    }, "Discarded the change");
    if (!state.prefs.prompts.discard) return void run();
    confirmDialog("Discard change", h("p", null, `Discard the captured change to ${labelOf(w)} in ${docOf(w.docId).filename}? It was never committed, so it cannot be brought back.`), "Discard", run, { danger: !0 });
  }
  function rowMenu(anchor, w) {
    openMenu(anchor, [
      { label: w.staged ? "Unstage" : "Stage", icon: w.staged ? "down" : "up", onSelect: () => {
        setStaged(state.db, [w.id], !w.staged), render();
      } },
      { label: `Open in ${HOST_LABEL[w.source]}`, icon: "external", onSelect: () => openInEditor(w.docId) },
      { label: "Copy block label", icon: "link", onSelect: () => void copy(labelOf(w)) },
      { label: "", separator: !0 },
      { label: "Discard change…", icon: "trash", danger: !0, onSelect: () => discardOne(w) }
    ]);
  }
  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text), toast("success", "Copied", text);
    } catch {
      toast("warning", "Copying is not available here");
    }
  }
  function changeRow(w) {
    let c = state.ui.changes, d = docOf(w.docId), [letter, word] = OP[w.edit.op];
    return h(
      "div",
      {
        class: cx("change-row", c.selected === w.id && "is-selected"),
        role: "option",
        "aria-selected": String(c.selected === w.id),
        tabindex: "0",
        "data-id": w.id,
        onClick: () => {
          c.selected = w.id, render();
        },
        onKeydown: (e) => {
          e.key === " " ? (e.preventDefault(), setStaged(state.db, [w.id], !w.staged), render()) : e.key === "Enter" && (c.selected = w.id, render());
        },
        onContextmenu: (e) => {
          e.preventDefault(), rowMenu(e.currentTarget, w);
        }
      },
      checkbox(w.staged, (v) => {
        setStaged(state.db, [w.id], v), render();
      }, `${w.staged ? "Unstage" : "Stage"} ${labelOf(w)}`),
      fileIcon(d.kind, 14),
      h("span", { class: "change-path" }, h("span", { class: "change-file" }, d.filename), h("span", { class: "change-sep" }, " › "), h("span", { class: "change-label" }, labelOf(w))),
      h("span", { class: cx("op", `op-${w.edit.op}`), title: word }, letter)
    );
  }
  function list(title, items, staged) {
    let all = items.length > 0;
    return h(
      "div",
      { class: "change-group" },
      h(
        "div",
        { class: "change-group-head" },
        checkbox(!!(staged && all), (v) => {
          setStaged(state.db, items.map((w) => w.id), v), render();
        }, staged ? "Unstage all" : "Stage all", { disabled: !all }),
        h("span", { class: "change-group-title" }, title),
        h("span", { class: "count" }, String(items.length)),
        h("span", { class: "spacer" }),
        all ? button(staged ? "Unstage all" : "Stage all", { size: "sm", variant: "ghost", onClick: () => {
          setStaged(state.db, items.map((w) => w.id), !staged), render();
        } }) : null
      ),
      h("div", { class: "change-list", role: "listbox", "aria-label": title }, items.length ? items.map(changeRow) : h("div", { class: "change-none" }, staged ? "Stage changes to commit them." : "Nothing unstaged."))
    );
  }
  function commitBox(staged) {
    let c = state.ui.changes, branch = currentBranch(), target = commitTarget(branchContext()), files = new Set(staged.map((w) => w.docId)).size, tooLong = state.prefs.prompts.commitLength && c.summary.length > 60, counter2 = h("span", { class: cx("commit-count", tooLong && "is-warn"), id: "commit-count" }, c.summary.length ? String(60 - c.summary.length) : ""), label = staged.length ? `Commit ${staged.length} change${staged.length === 1 ? "" : "s"} to ${branchLabel(branch)}` : `Commit to ${branchLabel(branch)}`;
    return h(
      "div",
      { class: "commit-box" },
      h(
        "div",
        { class: "commit-summary-row" },
        avatar(state.prefs.authorName),
        input(c.summary, (v) => {
          c.summary = v;
          let el = document.getElementById("commit-count");
          el && (el.textContent = v.length ? String(60 - v.length) : "", el.classList.toggle("is-warn", state.prefs.prompts.commitLength && v.length > 60));
          let btn = document.getElementById("commit-button");
          btn && (btn.disabled = !v.trim() || !staged.length || !target.ok);
        }, { id: "commit-summary", placeholder: "Summary (required)", maxlength: 200 }),
        counter2
      ),
      textarea(c.description, (v) => {
        c.description = v;
      }, {
        id: "commit-description",
        placeholder: "Description",
        rows: 3,
        onKeydown: (e) => {
          e.key === "Enter" && (e.ctrlKey || e.metaKey) && (e.preventDefault(), commitStaged2());
        }
      }),
      h(
        "div",
        { class: "commit-options" },
        h("label", { class: "inline-check is-disabled", title: "Dynodoc history is append-only: a revision cannot be changed after it is made." }, h("input", { type: "checkbox", class: "check", disabled: !0 }), "Amend last commit"),
        h("span", { class: "muted small" }, state.prefs.signCommits ? "Signed with this computer’s key" : "Unsigned")
      ),
      target.ok ? null : notice("info", target.note),
      button(label, { variant: "primary", id: "commit-button", class: "commit-button", disabled: !c.summary.trim() || !staged.length || !target.ok, onClick: commitStaged2 }),
      !target.ok && branch === "main" && staged.length ? button("Commit to a new draft…", { onClick: () => newDraftDialog(staged[0].docId), class: "commit-button" }) : null,
      staged.length ? h("div", { class: "muted small center" }, `${files} file${files === 1 ? "" : "s"} · one revision per file`) : null
    );
  }
  function nothingHere() {
    let ctx = branchContext(), cards = [];
    return (ctx.ahead ?? 0) > 0 && cards.push(h("div", { class: "suggest" }, h("div", null, h("div", { class: "suggest-title" }, `Push ${ctx.ahead} commit${ctx.ahead === 1 ? "" : "s"} to the server`), h("div", { class: "suggest-text" }, "Your commits are on this computer only until you push.")), button("Push", { variant: "primary", icon: "up", onClick: sync }))), ctx.kind === "draft" && ctx.state === "private" && !(ctx.ahead ?? 0) && cards.push(h("div", { class: "suggest" }, h("div", null, h("div", { class: "suggest-title" }, "Send this draft for review"), h("div", { class: "suggest-text" }, "Open a change request so the team can review and merge it.")), button("Send for review", { onClick: () => sendForReviewDialog() }))), cards.push(h("div", { class: "suggest" }, h("div", null, h("div", { class: "suggest-title" }, "Fetch from the server"), h("div", { class: "suggest-text" }, "See new revisions, reviews and change requests.")), button("Fetch", { icon: "sync", onClick: fetchNow }))), cards.push(h("div", { class: "suggest" }, h("div", null, h("div", { class: "suggest-title" }, "Edit in your editor"), h("div", { class: "suggest-text" }, "Editing happens in Word, Excel, PowerPoint or Google’s editors with the Dynodoc add-in. Captured edits appear here.")), button("Open a file…", { onClick: () => {
      state.ui.view = "documents", render();
    } }))), h("div", { class: "no-changes" }, empty("check", "No captured changes", `Nothing waiting on ${branchLabel()}. Here are some friendly suggestions for what to do next.`), h("div", { class: "suggests" }, cards));
  }
  function changesView() {
    let c = state.ui.changes, branch = currentBranch(), all = working(state.db, state.ui.projectId, branch), q = c.filter.toLowerCase(), shown = all.filter((w) => !q || `${docOf(w.docId).filename} ${labelOf(w)}`.toLowerCase().includes(q)), unstaged = shown.filter((w) => !w.staged), staged = shown.filter((w) => w.staged);
    (!c.selected || !all.some((w) => w.id === c.selected)) && (c.selected = (staged[0] ?? unstaged[0])?.id ?? null);
    let sel = all.find((w) => w.id === c.selected), left = h(
      "div",
      { class: "pane pane-list changes-pane" },
      h("div", { class: "pane-filter" }, icon("filter", 14), input(c.filter, (v) => {
        c.filter = v, render();
      }, { id: "changes-filter", placeholder: "Filter changes" })),
      h("div", { class: "pane-scroll", "data-scroll-key": "changes-list" }, list("Unstaged changes", unstaged, !1), list("Staged changes", staged, !0)),
      commitBox(all.filter((w) => w.staged))
    ), right;
    if (!all.length) right = h("div", { class: "pane pane-main" }, nothingHere());
    else if (!sel) right = h("div", { class: "pane pane-main" }, empty("changes", "Select a change", "Choose a change at the left to see its diff."));
    else {
      let d = docOf(sel.docId), base = blocksOf(state.db, sel.docId, branch), docChanges = all.filter((w) => w.docId === sel.docId), st = stats(docChanges.map((w) => w.edit)), stagedIds = new Set(docChanges.filter((w) => w.staged).map((w) => w.edit.blockId));
      right = h(
        "div",
        { class: "pane pane-main" },
        h(
          "div",
          { class: "diff-head" },
          fileIcon(d.kind),
          h("span", { class: "diff-path" }, h("strong", null, d.filename), h("span", { class: "muted" }, ` on ${branchLabel(branch)}`)),
          h("span", { class: "diff-stat" }, h("span", { class: "plus" }, `+${st.wordsAdded}`), " ", h("span", { class: "minus" }, `−${st.wordsRemoved}`), h("span", { class: "muted" }, " words")),
          h("span", { class: "spacer" }),
          h("span", { class: "muted small" }, `Captured by ${HOST_LABEL[sel.source]} · ${short(sel.capturedAt)}`),
          segmented(state.prefs.diffLayout, [["unified", "", "unified"], ["split", "", "split"]], (v) => {
            state.prefs.diffLayout = v, render();
          }, "Diff layout"),
          select(String(state.prefs.diffContext), [["0", "No context"], ["1", "1 block"], ["2", "2 blocks"], ["3", "3 blocks"], ["5", "5 blocks"], ["999", "Whole file"]], (v) => {
            state.prefs.diffContext = Number(v), render();
          }, { label: "Context", class: "select-sm" }),
          iconButton("external", `Open in ${HOST_LABEL[sel.source]}`, () => openInEditor(sel.docId)),
          iconButton("trash", "Discard this change", () => discardOne(sel))
        ),
        h(
          "div",
          { class: "pane-scroll diff-scroll", "data-scroll-key": "changes-diff" },
          diffView(base, docChanges.map((w) => w.edit), {
            excluded: docChanges.filter((w) => !stagedIds.has(w.edit.blockId)).map((w) => w.edit.blockId),
            onToggle: (blockId, include) => {
              let w = docChanges.find((x) => x.edit.blockId === blockId);
              w && setStaged(state.db, [w.id], include), render();
            }
          }),
          h("p", { class: "muted small diff-foot" }, "A tick stages the change; dimmed changes are not staged. Captured changes stay on this computer until you commit and push.")
        )
      );
    }
    return h("div", { class: "split-view" }, left, right);
  }

  // client/src/views/history.ts
  function commitsForView() {
    let ui = state.ui.history, branch = currentBranch(), docs = new Set(docsOf(state.db, state.ui.projectId).map((d) => d.id)), list2;
    if (branch === "main") list2 = state.db.commits.filter((c) => docs.has(c.docId) && c.branch === "main");
    else {
      let d = draft(state.db, branch);
      list2 = d ? [...state.db.commits.filter((c) => c.branch === d.id), ...state.db.commits.filter((c) => c.docId === d.docId && c.branch === "main" && c.seq <= d.baseSeq)] : [];
    }
    ui.docFilter && (list2 = list2.filter((c) => c.docId === ui.docFilter));
    let q = ui.filter.toLowerCase();
    return q && (list2 = list2.filter((c) => `${c.summary} ${personName(c.authorId)} ${c.chainHash.slice(0, 12)} ${rev(c)}`.toLowerCase().includes(q))), list2.sort((a, b) => b.at - a.at);
  }
  function recover(c, mode) {
    let run = () => act(() => {
      let d = recoveryDraft(state.db, c.id, mode);
      return state.ui.branchByProject[state.ui.projectId] = d.id, state.ui.history.selected = null, `${mode === "revert" ? "Reverted" : "Picked"} ${rev(c)} into the draft ${d.name}. Review it, then send it for review.`;
    });
    if (!state.prefs.prompts.recovery) return void run();
    confirmDialog(
      mode === "revert" ? "Revert this revision" : "Cherry-pick this revision",
      h("p", null, mode === "revert" ? `Dynodoc never rewrites history. Reverting ${rev(c)} "${c.summary}" makes a new draft of ${docOf(c.docId).filename} that undoes its changes; it reaches main only through a change request.` : `Cherry-picking ${rev(c)} "${c.summary}" makes a new draft of ${docOf(c.docId).filename} from the latest main with these changes applied again.`),
      mode === "revert" ? "Create revert draft" : "Create draft",
      run
    );
  }
  function commitMenu(anchor, c) {
    openMenu(anchor, [
      { label: "Revert this revision…", icon: "revert", disabled: c.local, note: c.local ? "Push it first." : void 0, onSelect: () => recover(c, "revert") },
      { label: "Cherry-pick onto a new draft…", icon: "cherry", disabled: c.branch === "main" && c.kind !== "revert", note: c.branch === "main" && c.kind !== "revert" ? "Already on main." : void 0, onSelect: () => recover(c, "cherry_pick") },
      { label: "Verify hash chain of this file", icon: "shield", onSelect: () => verifyFile(c.docId) },
      { label: "", separator: !0 },
      { label: "Copy chain hash", icon: "link", onSelect: () => void navigator.clipboard?.writeText(c.chainHash).catch(() => {
      }) },
      { label: "Reset to this revision", disabled: !0, note: "Dynodoc history is append-only; restore content through a draft instead." },
      { label: "Amend", disabled: !0, note: "Revisions cannot be changed after they are made." }
    ]);
  }
  function verifyFile(docId) {
    let r = runVerify(docId);
    act(() => r.bad ? `${r.bad} revision${r.bad === 1 ? "" : "s"} of ${docOf(docId).filename} do not verify` : `Verified: the hash chain of ${docOf(docId).filename} is intact (${r.ok} revisions)`);
  }
  function commitRow(c, selected) {
    let d = docOf(c.docId);
    return h(
      "div",
      {
        class: cx("commit-row", selected && "is-selected"),
        role: "option",
        tabindex: "0",
        "aria-selected": String(selected),
        onClick: () => {
          state.ui.history.selected = c.id, render();
        },
        onKeydown: (e) => {
          e.key === "Enter" && (state.ui.history.selected = c.id, render()), (e.key === "ArrowDown" || e.key === "ArrowUp") && (e.preventDefault(), e.currentTarget[e.key === "ArrowDown" ? "nextElementSibling" : "previousElementSibling"]?.click(), requestAnimationFrame(() => document.querySelector(".commit-row.is-selected")?.focus()));
        },
        onContextmenu: (e) => {
          e.preventDefault(), commitMenu(e.currentTarget, c);
        }
      },
      h("div", { class: "commit-line1" }, c.kind === "merge" ? icon("merge", 13, "icon commit-kind") : c.kind === "revert" ? icon("revert", 13, "icon commit-kind") : null, h("span", { class: "commit-summary" }, c.summary)),
      h(
        "div",
        { class: "commit-line2" },
        avatar(personName(c.authorId), isAgent(c.authorId)),
        h("span", { class: "commit-author" }, personName(c.authorId)),
        h("span", { class: "muted" }, short(c.at)),
        h("span", { class: "spacer" }),
        fileIcon(d.kind, 12),
        h("span", { class: "commit-rev mono" }, rev(c)),
        c.local ? h("span", { class: "local-badge", title: "On this computer only: push to share it" }, icon("up", 11)) : null,
        verifyIcon(c.id)
      )
    );
  }
  function commitDetail(c) {
    let d = docOf(c.docId), base = commitBase(c), st = stats(c.edits), p2 = state.prefs;
    return h(
      "div",
      { class: "pane pane-main" },
      h(
        "div",
        { class: "commit-head" },
        h("div", { class: "commit-head-title" }, h("h2", null, c.summary), c.description ? h("p", { class: "commit-desc" }, c.description) : null),
        h(
          "div",
          { class: "commit-meta" },
          avatar(personName(c.authorId), isAgent(c.authorId), "md"),
          h("span", null, h("strong", null, personName(c.authorId)), isAgent(c.authorId) ? h("span", { class: "muted" }, " (agent key)") : null),
          h("span", { class: "muted" }, `${formatDate(c.at, p2)} ${formatTime(c.at, p2)}`),
          h("span", { class: "meta-sep" }),
          fileIcon(d.kind, 13),
          h("span", null, d.filename),
          h("span", { class: "mono" }, rev(c)),
          c.kind !== "revision" ? pill(c.kind.replace("_", "-"), c.kind === "merge" ? "primary" : "info") : null,
          c.local ? pill("not pushed", "warning") : null,
          h("span", { class: "spacer" }),
          button("", { icon: "more", variant: "ghost", size: "sm", title: "More", onClick: (e) => commitMenu(e.currentTarget, c) })
        ),
        h(
          "dl",
          { class: "chain" },
          h("dt", null, "Chain hash"),
          h("dd", null, hash(c.chainHash, 64)),
          h("dt", null, "Previous"),
          h("dd", null, hash(c.prevChainHash, 64)),
          h("dt", null, "Content hash"),
          h("dd", null, hash(c.contentHash, 64)),
          h("dt", null, "Verification"),
          h("dd", null, verifyPill(c.id), " ", button("Verify hash chain", { size: "sm", icon: "shield", onClick: () => verifyFile(c.docId) }))
        ),
        h(
          "div",
          { class: "commit-actions" },
          h("span", { class: "diff-stat" }, `${st.edited} edited · ${st.added} added · ${st.removed} removed`),
          h("span", { class: "spacer" }),
          segmented(p2.diffLayout, [["unified", "Unified"], ["split", "Split"]], (v) => {
            p2.diffLayout = v, render();
          }, "Diff layout"),
          button("Revert", { size: "sm", icon: "revert", disabled: c.local, title: "Make a draft that undoes this revision", onClick: () => recover(c, "revert") }),
          c.branch !== "main" || c.kind === "revert" ? button("Cherry-pick", { size: "sm", icon: "cherry", onClick: () => recover(c, "cherry_pick") }) : null
        )
      ),
      h("div", { class: "pane-scroll diff-scroll", "data-scroll-key": "history-diff" }, diffView(base, c.edits, { emptyText: "This revision changes nothing." }))
    );
  }
  function compareView() {
    let ui = state.ui.history, drafts2 = state.db.drafts.filter((d2) => d2.projectId === state.ui.projectId && (d2.state === "private" || d2.state === "in_review"));
    (!ui.compareBranch || !drafts2.some((d2) => d2.id === ui.compareBranch)) && (ui.compareBranch = (drafts2.find((d2) => d2.id === currentBranch()) ?? drafts2[0])?.id ?? "");
    let d = draft(state.db, ui.compareBranch), head = h(
      "div",
      { class: "compare-head" },
      h("span", { class: "muted" }, "Compare"),
      select(ui.compareBranch, drafts2.map((x) => [x.id, `${x.name} (${docOf(x.docId).filename})`]), (v) => {
        ui.compareBranch = v, render();
      }, { label: "Draft to compare" }),
      h("span", { class: "muted" }, "with"),
      h("span", { class: "branch-chip" }, icon("branch", 13), "main")
    );
    if (!d) return h("div", { class: "pane pane-main" }, head, empty("compare", "No open drafts", "Start a draft from the Branch menu to compare it with main."));
    let ahead = branchCommits(state.db, d.docId, d.id).length, behind2 = behind(state.db, d), edits2 = draftEdits(state.db, d), conflicts2 = conflictsOf(state.db, d), mine = d.authorId === state.db.me;
    return h(
      "div",
      { class: "pane pane-main" },
      head,
      h(
        "div",
        { class: "compare-summary" },
        pill(`${ahead} ahead`, "primary"),
        pill(`${behind2} behind`, behind2 ? "warning" : "neutral"),
        pill(draftWord(d.state), draftTone(d.state)),
        conflicts2.length ? pill(`${conflicts2.length} conflict${conflicts2.length === 1 ? "" : "s"}`, "error") : null,
        h("span", { class: "spacer" }),
        mine && behind2 ? button("Update from main…", { size: "sm", onClick: () => rebaseDialog(d.id, "update") }) : null,
        mine && d.state === "private" ? button("Send for review", { size: "sm", variant: "primary", onClick: () => sendForReviewDialog(d.id) }) : null,
        currentBranch() !== d.id ? button("Switch to this draft", { size: "sm", onClick: () => switchBranch(d.id) }) : null
      ),
      conflicts2.length ? notice("warning", `main changed ${conflicts2.map((c) => c.label).join(", ")} after this draft started. Rebase or resolve them in the review.`) : null,
      h("div", { class: "pane-scroll diff-scroll", "data-scroll-key": "compare-diff" }, diffView(baseBlocks(state.db, d), edits2, { emptyText: "The draft has no changes against main." }))
    );
  }
  function graphView() {
    let ui = state.ui.history, docs = docsOf(state.db, state.ui.projectId), docId = ui.docFilter || docs[0]?.id, head = h(
      "div",
      { class: "compare-head" },
      h("span", { class: "muted" }, "History graph of"),
      select(docId, docs.map((x2) => [x2.id, x2.filename]), (v) => {
        ui.docFilter = v, render();
      }, { label: "File" }),
      h("span", { class: "spacer" }),
      h("span", { class: "legend" }, h("span", { class: "lg lg-main" }), "main", h("span", { class: "lg lg-draft" }), "drafts")
    );
    if (!docId) return h("div", { class: "pane pane-main" }, head);
    let drafts2 = state.db.drafts.filter((d) => d.docId === docId && branchCommits(state.db, docId, d.id).length > 0), lanes = new Map([["main", 0], ...drafts2.map((d, i) => [d.id, i + 1])]), commits = state.db.commits.filter((c) => c.docId === docId && lanes.has(c.branch)).sort((a, b) => b.at - a.at), ROW = 44, LANE = 22, X0 = 18, y = (i) => i * ROW + ROW / 2, x = (lane) => X0 + lane * LANE, idx = new Map(commits.map((c, i) => [c.id, i])), width = X0 + (drafts2.length + 1) * LANE, height = commits.length * ROW, paths = [], mainRows = commits.filter((c) => c.branch === "main").map((c) => idx.get(c.id));
    mainRows.length && paths.push(s("path", { d: `M${x(0)} ${y(Math.min(...mainRows))} V${y(Math.max(...mainRows))}`, class: "g-main" }));
    for (let d of drafts2) {
      let lane = lanes.get(d.id), rows3 = commits.filter((c) => c.branch === d.id).map((c) => idx.get(c.id)), top = Math.min(...rows3), bottom = Math.max(...rows3);
      paths.push(s("path", { d: `M${x(lane)} ${y(top)} V${y(bottom)}`, class: "g-draft" }));
      let base = commits.find((c) => c.branch === "main" && c.seq === d.baseSeq);
      if (base) {
        let by = y(idx.get(base.id));
        paths.push(s("path", { d: `M${x(lane)} ${y(bottom)} C${x(lane)} ${by - 10}, ${x(0)} ${by - 14}, ${x(0)} ${by}`, class: "g-draft" }));
      }
      let merge2 = commits.find((c) => c.branch === "main" && c.mergedFrom === d.id);
      if (merge2) {
        let my = y(idx.get(merge2.id));
        paths.push(s("path", { d: `M${x(lane)} ${y(top)} C${x(lane)} ${my + 10}, ${x(0)} ${my + 14}, ${x(0)} ${my}`, class: "g-draft" }));
      } else (d.state === "in_review" || d.state === "private") && paths.push(s("path", { d: `M${x(lane)} ${y(top)} V${Math.max(4, y(top) - ROW / 2)}`, class: "g-draft g-open" }));
    }
    let dots = commits.map((c, i) => s("circle", { cx: x(lanes.get(c.branch)), cy: y(i), r: c.kind === "merge" ? 5.5 : 4.5, class: cx("g-dot", c.branch === "main" ? "g-dot-main" : "g-dot-draft", c.kind === "merge" && "g-dot-merge") })), svg = s("svg", { class: "graph-svg", width, height, viewBox: `0 0 ${width} ${height}`, "aria-hidden": "true" }, paths, dots);
    return h(
      "div",
      { class: "pane pane-main" },
      head,
      h(
        "div",
        { class: "pane-scroll", "data-scroll-key": "graph" },
        h(
          "div",
          { class: "graph", css: { "--graph-w": `${width}px` } },
          svg,
          h(
            "div",
            { class: "graph-rows" },
            commits.map(
              (c) => h(
                "button",
                { type: "button", class: cx("graph-row", state.ui.history.selected === c.id && "is-selected"), onClick: () => {
                  state.ui.history.selected = c.id, state.ui.history.mode = "list", c.branch !== currentBranch() && c.branch !== "main" && switchBranch(c.branch), render();
                } },
                h("span", { class: "graph-summary" }, c.summary),
                c.branch !== "main" ? h("span", { class: "branch-chip" }, icon("branch", 11), branchName(state.db, c.branch)) : null,
                h("span", { class: "spacer" }),
                h("span", { class: "muted" }, personName(c.authorId)),
                h("span", { class: "mono muted" }, rev(c)),
                h("span", { class: "mono muted" }, c.chainHash.slice(0, 7)),
                h("span", { class: "muted" }, short(c.at))
              )
            )
          )
        )
      )
    );
  }
  function historyView() {
    let ui = state.ui.history, docs = docsOf(state.db, state.ui.projectId), list2 = commitsForView();
    ui.mode === "list" && (!ui.selected || !list2.some((c) => c.id === ui.selected)) && (ui.selected = list2[0]?.id ?? null);
    let sel = list2.find((c) => c.id === ui.selected), left = h(
      "div",
      { class: "pane pane-list" },
      h("div", { class: "pane-tabs" }, tabs(ui.mode, [["list", "History"], ["compare", "Compare"], ["graph", "Graph"]], (v) => {
        ui.mode = v, render();
      }, "History mode")),
      h("div", { class: "pane-filter" }, icon("search", 14), input(ui.filter, (v) => {
        ui.filter = v, render();
      }, { id: "history-filter", placeholder: "Filter by summary, author, hash" })),
      h("div", { class: "pane-filter" }, select(ui.docFilter, [["", "All files"], ...docs.map((d) => [d.id, d.filename])], (v) => {
        ui.docFilter = v, render();
      }, { label: "File", class: "select-block" })),
      h("div", { class: "pane-sub muted small" }, `${list2.length} revision${list2.length === 1 ? "" : "s"} on ${branchLabel()}${currentBranch() !== "main" ? " (and main before it)" : ""}`),
      h("div", { class: "pane-scroll", role: "listbox", "aria-label": "Revisions", "data-scroll-key": "history-list" }, list2.length ? list2.map((c) => commitRow(c, c.id === ui.selected)) : h("div", { class: "change-none" }, "No revisions match.")),
      h("div", { class: "pane-foot" }, button("Verify every file", { icon: "shield", size: "sm", onClick: () => {
        let bad = 0, ok = 0;
        for (let d of docs) {
          let r = runVerify(d.id);
          bad += r.bad, ok += r.ok;
        }
        act(() => bad ? `${bad} revisions do not verify` : `Verified ${ok} revisions in ${docs.length} files: every chain is intact`);
      } }))
    ), right;
    return ui.mode === "compare" ? right = compareView() : ui.mode === "graph" ? right = graphView() : right = sel ? commitDetail(sel) : h("div", { class: "pane pane-main" }, empty("history", "No revisions", "Nothing has been committed here yet.")), h("div", { class: "split-view" }, left, right);
  }

  // client/src/views/requests.ts
  function listFor(filter) {
    return state.db.requests.filter((r) => r.projectId === state.ui.projectId).filter((r) => filter === "open" ? r.status === "open" : filter === "merged" ? r.status === "merged" : r.status === "declined" || r.status === "withdrawn").sort((a, b) => b.createdAt - a.createdAt);
  }
  function reqRow(r) {
    let st = reviewState(state.db, r), sel = state.ui.requests.selected === r.id;
    return h(
      "button",
      { type: "button", class: cx("req-row", sel && "is-selected"), "aria-pressed": String(sel), onClick: () => {
        state.ui.requests.selected = r.id, render();
      } },
      h("div", { class: "req-line1" }, icon(r.status === "merged" ? "merge" : "request", 14, `icon req-icon st-${r.status}`), h("span", { class: "req-title" }, r.name), h("span", { class: "muted mono" }, `#${r.number}`)),
      h(
        "div",
        { class: "req-line2" },
        fileIcon(docOf(r.docId).kind, 12),
        h("span", { class: "muted" }, docOf(r.docId).filename),
        h("span", { class: "muted" }, "·"),
        avatar(personName(r.authorId), isAgent(r.authorId)),
        h("span", { class: "muted" }, personName(r.authorId)),
        r.byAgent ? badge("AI proposal", "info") : null,
        h("span", { class: "spacer" }),
        r.status === "open" ? h("span", { class: cx("req-approvals", st.satisfied && "is-ok"), title: st.why }, icon("check", 12), `${st.approvals}/${st.required}`) : null,
        r.status === "open" && r.conflicts.length ? h("span", { class: "req-conflict", title: `${r.conflicts.length} conflicts` }, icon("warning", 12), String(r.conflicts.length)) : null,
        h("span", { class: "muted" }, short(r.createdAt))
      )
    );
  }
  function mergeDialog(r) {
    let included = r.edits.filter((e) => !r.excluded.includes(e.blockId)), target = mainSeq(state.db, r.docId), run = () => act(() => {
      let c = merge(state.db, r.id);
      return `Merged #${r.number} into main as r${c.seq}`;
    });
    if (!state.prefs.prompts.merge) return void run();
    openDialog({
      title: `Merge #${r.number}`,
      plain: !0,
      width: 520,
      body: () => h(
        "dl",
        { class: "facts" },
        h("dt", null, "Request"),
        h("dd", null, `#${r.number} ${r.name}`),
        h("dt", null, "File"),
        h("dd", null, docOf(r.docId).filename),
        h("dt", null, "Into"),
        h("dd", null, `main, r${target} becomes r${target + 1}`),
        h("dt", null, "Changes"),
        h("dd", null, `${included.length} included${r.excluded.length ? `, ${r.excluded.length} left out` : ""}`),
        r.conflicts.length ? [h("dt", null, "Conflicts"), h("dd", null, r.conflicts.map((c) => `${c.label}: ${c.choice === "team" ? "team's version" : "their version"}`).join("; "))] : null,
        h("dt", null, "Approvals"),
        h("dd", null, `${reviewState(state.db, r).approvals} of ${reviewState(state.db, r).required} required`),
        h("dt", null, "Merged by"),
        h("dd", null, `${state.prefs.authorName} (you), on this computer`)
      ),
      actions: () => [
        button("Cancel", { onClick: closeDialog }),
        button(`Merge ${included.length} change${included.length === 1 ? "" : "s"}`, { variant: "primary", id: "dialog-confirm", onClick: () => {
          closeDialog(), run();
        } })
      ]
    });
  }
  function declineDialog(r) {
    let note = "";
    openDialog({
      title: `Decline #${r.number}`,
      plain: !0,
      width: 520,
      body: () => [h("p", { class: "dialog-text" }, "The request is closed without merging. Its draft and comments stay in history."), textarea(note, (v) => {
        note = v;
      }, { id: "decline-note", placeholder: "Why (shown to the author)", rows: 3 })],
      actions: () => [button("Cancel", { onClick: closeDialog }), button("Decline request", { variant: "danger", onClick: () => {
        act(() => decline(state.db, r.id, note), `Declined #${r.number}`) && closeDialog();
      } })]
    });
  }
  function commentDialog(r, blockId) {
    let text = "", label = blocksOf(state.db, r.docId, r.draftId).find((b) => b.id === blockId)?.label ?? blockId;
    openDialog({
      title: `Comment on ${label}`,
      width: 520,
      body: () => textarea(text, (v) => {
        text = v;
      }, { id: "block-comment", rows: 4, placeholder: "Leave a comment on this change" }),
      actions: () => [button("Cancel", { onClick: closeDialog }), button("Comment", { variant: "primary", onClick: () => {
        act(() => comment(state.db, r.id, text, blockId), "Comment added") && closeDialog();
      } })]
    });
  }
  function commentItem(c) {
    return h(
      "div",
      { class: "comment" },
      avatar(personName(c.authorId), isAgent(c.authorId)),
      h("div", { class: "comment-body" }, h("div", { class: "comment-meta" }, h("strong", null, personName(c.authorId)), h("span", { class: "muted" }, when(c.at)), c.proposedByAgent ? badge("drafted by the agent, posted by a person", "info") : null), h("div", { class: "comment-text" }, c.text))
    );
  }
  function review2(r) {
    var _a;
    let st = reviewState(state.db, r), p2 = project(state.db, r.projectId), d = draft(state.db, r.draftId), doc2 = docOf(r.docId), behind2 = d ? behind(state.db, d) : 0, base = d ? baseBlocks(state.db, d) : doc2.blocks, byBlock = {};
    for (let c of r.comments) c.blockId && (byBlock[_a = c.blockId] ?? (byBlock[_a] = [])).push(commentItem(c));
    let open2 = r.status === "open", mineReq = r.authorId === state.db.me, latest = /* @__PURE__ */ new Map();
    for (let rv of r.reviews) latest.set(rv.reviewerId, rv);
    let conflicts2 = r.conflicts.length && open2 ? h(
      "div",
      { class: "card conflicts-card" },
      h(
        "div",
        { class: "card-head" },
        icon("warning", 15),
        h("strong", null, `${r.conflicts.length} conflict${r.conflicts.length === 1 ? "" : "s"} with main`),
        h("span", { class: "spacer" }),
        button("Keep all team's", { size: "sm", onClick: () => {
          r.conflicts.forEach((c) => c.choice = "team"), render();
        } }),
        button("Use all of theirs", { size: "sm", onClick: () => {
          r.conflicts.forEach((c) => c.choice = "draft"), render();
        } })
      ),
      h("p", { class: "muted small" }, "main changed these blocks after the draft started. Choose what the merge keeps for each."),
      r.conflicts.map(
        (c) => h(
          "div",
          { class: "conflict" },
          h("div", { class: "conflict-label" }, c.label),
          h("div", { class: "conflict-ancestor" }, h("span", { class: "muted" }, "Before both: "), c.ancestor),
          h(
            "div",
            { class: "conflict-choices" },
            ["team", "draft"].map(
              (side) => h(
                "label",
                { class: cx("conflict-choice", c.choice === side && "is-on") },
                h("input", { type: "radio", name: `rc-${r.id}-${c.blockId}`, checked: c.choice === side, onChange: () => {
                  c.choice = side, render();
                } }),
                h("span", { class: "conflict-side" }, side === "team" ? "Keep the team's current version" : `Use ${personName(r.authorId)}'s version`),
                h("span", { class: "conflict-text" }, side === "team" ? c.team : c.draft)
              )
            )
          )
        )
      )
    ) : null;
    return h(
      "div",
      { class: "pane pane-main" },
      h(
        "div",
        { class: "req-head" },
        h("div", { class: "req-head-line" }, h("h2", null, r.name, " ", h("span", { class: "muted" }, `#${r.number}`)), pill(r.status, requestTone(r.status))),
        h(
          "div",
          { class: "req-sub" },
          avatar(personName(r.authorId), isAgent(r.authorId)),
          h("span", null, h("strong", null, personName(r.authorId)), ` wants to merge ${r.edits.length} change${r.edits.length === 1 ? "" : "s"} into `, h("span", { class: "branch-chip" }, icon("branch", 11), "main"), " from ", h("span", { class: "branch-chip" }, icon("branch", 11), d?.name ?? "a draft")),
          h("span", { class: "muted" }, `· ${doc2.filename} · based on r${r.baseSeq} · ${when(r.createdAt)}`),
          d && d.authorId === state.db.me && (d.state === "in_review" || d.state === "private") ? button("Switch to draft", { size: "sm", variant: "ghost", onClick: () => switchBranch(d.id) }) : null
        )
      ),
      h(
        "div",
        { class: "pane-scroll req-scroll", "data-scroll-key": `req-${r.id}` },
        r.byAgent ? notice("info", "Opened with an agent key (an AI proposal). The agent cannot approve or merge it; a person reviews it like any other request.") : null,
        r.agentStatement ? untrusted("What the agent said (not verified by Dynodoc)", r.agentStatement) : null,
        r.note ? h("div", { class: "req-note" }, r.note) : null,
        open2 && behind2 ? notice("warning", `The team version has ${behind2} newer revision${behind2 === 1 ? "" : "s"} since this draft started.${r.conflicts.length ? "" : " None of them touch the same blocks."}`) : null,
        conflicts2,
        h("div", { class: "section-title" }, "Changes", h("span", { class: "muted small" }, open2 ? " · untick a change to leave it out of the merge" : "")),
        diffView(base, r.edits, {
          excluded: r.excluded,
          onToggle: open2 ? (blockId, include) => {
            r.excluded = include ? r.excluded.filter((x) => x !== blockId) : [...r.excluded, blockId], render();
          } : void 0,
          comments: byBlock,
          onComment: open2 ? (blockId) => commentDialog(r, blockId) : void 0
        }),
        h("div", { class: "section-title" }, "Conversation"),
        h("div", { class: "comments" }, r.comments.filter((c) => !c.blockId).map(commentItem), r.comments.filter((c) => !c.blockId).length ? null : h("div", { class: "muted small" }, "No general comments yet."))
      ),
      h(
        "div",
        { class: "review-bar" },
        h(
          "div",
          { class: "review-state" },
          h("div", { class: "review-rule" }, icon(st.satisfied ? "check" : "info", 14), h("strong", null, `${st.approvals} of ${st.required} approval${st.required === 1 ? "" : "s"}`), h("span", { class: "muted" }, ` · ${p2.rules.mergeRoles === "owners" ? "owners and managers merge" : "editors and above merge"}`)),
          h(
            "div",
            { class: "reviewers" },
            [...latest.values()].map(
              (rv) => h("span", { class: cx("reviewer", `v-${rv.verdict}`, rv.stale && "is-stale"), title: rv.stale ? "Given before the latest changes" : rv.note }, avatar(personName(rv.reviewerId)), personName(rv.reviewerId), " ", rv.verdict === "approved" ? "approved" : rv.verdict === "changes_requested" ? "requested changes" : "commented", rv.stale ? " (stale)" : "")
            ),
            latest.size ? null : h("span", { class: "muted small" }, "No reviews yet.")
          ),
          h("div", { class: "muted small" }, st.why)
        ),
        open2 ? h(
          "div",
          { class: "review-compose" },
          textarea(state.ui.requests.comment, (v) => {
            state.ui.requests.comment = v;
          }, { id: "review-comment", placeholder: mineReq ? "Reply to the reviewers" : "Leave a review comment", rows: 2 }),
          h(
            "div",
            { class: "review-buttons" },
            button("Comment", { size: "sm", onClick: () => {
              act(() => review(state.db, r.id, "commented", state.ui.requests.comment), "Comment added") && (state.ui.requests.comment = "");
            } }),
            button("Request changes", { size: "sm", disabled: !st.canApprove, title: st.canApprove ? "" : mineReq ? "You cannot review your own request." : `A ${ROLE_LABEL[p2.role]} cannot review.`, onClick: () => {
              act(() => review(state.db, r.id, "changes_requested", state.ui.requests.comment), "Requested changes") && (state.ui.requests.comment = "");
            } }),
            button("Approve", { size: "sm", icon: "check", disabled: !st.canApprove, title: st.canApprove ? "" : mineReq ? "You cannot review your own request." : `A ${ROLE_LABEL[p2.role]} cannot approve.`, onClick: () => {
              act(() => review(state.db, r.id, "approved", state.ui.requests.comment), `Approved #${r.number}`) && (state.ui.requests.comment = "");
            } }),
            h("span", { class: "spacer" }),
            mineReq ? button("Withdraw", { size: "sm", variant: "ghost", onClick: () => act(() => withdraw(state.db, r.draftId), `Withdrew #${r.number}`) }) : button("Decline", { size: "sm", variant: "ghost", onClick: () => declineDialog(r) }),
            button("Merge into main", { variant: "primary", icon: "merge", disabled: !st.canMerge, title: st.why, onClick: () => mergeDialog(r) })
          )
        ) : h("div", { class: "muted small" }, r.status === "merged" && r.mergedAt ? `Merged ${when(r.mergedAt)}.` : `This request is ${r.status}.`)
      )
    );
  }
  function requestsView() {
    let ui = state.ui.requests, counts = { open: listFor("open").length, merged: listFor("merged").length, closed: listFor("closed").length }, list2 = listFor(ui.filter);
    (!ui.selected || !state.db.requests.some((r) => r.id === ui.selected && r.projectId === state.ui.projectId)) && (ui.selected = list2[0]?.id ?? null);
    let sel = state.db.requests.find((r) => r.id === ui.selected), left = h(
      "div",
      { class: "pane pane-list" },
      h("div", { class: "pane-tabs" }, tabs(ui.filter, [["open", "Open", counts.open], ["merged", "Merged", counts.merged], ["closed", "Closed", counts.closed]], (v) => {
        ui.filter = v, ui.selected = null, render();
      }, "Request state")),
      h("div", { class: "pane-scroll", "data-scroll-key": "req-list" }, list2.length ? list2.map(reqRow) : h("div", { class: "change-none" }, `No ${ui.filter} change requests.`))
    ), right = sel ? review2(sel) : h("div", { class: "pane pane-main" }, empty("request", "No change request selected", "Drafts become change requests when they are sent for review."));
    return h("div", { class: "split-view" }, left, right);
  }

  // client/src/views/projects.ts
  function newProjectDialog() {
    let name2 = "", description = "";
    openDialog({
      title: "New project",
      width: 520,
      body: () => [
        field("Project name", input(name2, (v) => {
          name2 = v;
        }, { id: "np-name", autofocus: !0, maxlength: 100 })),
        field("Description", textarea(description, (v) => {
          description = v;
        }, { id: "np-desc", rows: 2 })),
        h("p", { class: "muted small" }, "Private: only invited people can access this project.")
      ],
      actions: () => [
        button("Cancel", { onClick: closeDialog }),
        button("Create project", {
          variant: "primary",
          onClick: () => {
            if (!name2.trim()) return toast("warning", "Name the project");
            let p2 = {
              id: `p_${Date.now().toString(36)}`,
              name: name2.trim(),
              description: description.trim(),
              ownerId: state.db.me,
              role: "owner",
              private: !0,
              starred: !1,
              stars: 0,
              watch: "requests",
              updatedAt: Date.now(),
              rules: { protectMain: !0, requiredApprovals: 1, mergeRoles: "editors" },
              members: [{ personId: state.db.me, role: "owner", via: "direct" }],
              issues: [],
              labels: [],
              agentPolicy: { enabled: !1, minRole: "editor", callsPerMinute: 30, openProposals: 3, proposalsPerDay: 10, submitRequiresPerson: !0 },
              space: "sp_org"
            };
            state.db.projects.push(p2), state.ui.branchByProject[p2.id] = "main", state.ui.projects.selected = p2.id, closeDialog(), toast("success", `Created ${p2.name}`, "Add files from the web workspace or an add-in.");
          }
        })
      ]
    });
  }
  function projectCard(p2) {
    let docs = docsOf(state.db, p2.id), open2 = state.db.requests.filter((r) => r.projectId === p2.id && r.status === "open").length;
    return h(
      "div",
      { class: cx("project-card", state.ui.projectId === p2.id && "is-current"), tabindex: "0", role: "button", onClick: () => {
        state.ui.projects.selected = p2.id, render();
      }, onKeydown: (e) => {
        e.key === "Enter" && (state.ui.projects.selected = p2.id, render());
      } },
      h("div", { class: "project-card-head" }, icon("repo", 16), h("strong", null, p2.name), p2.private ? badge("Private") : null, h("span", { class: "spacer" }), h("button", { type: "button", class: cx("star", p2.starred && "is-on"), "aria-label": p2.starred ? "Unstar" : "Star", "aria-pressed": String(p2.starred), onClick: (e) => {
        e.stopPropagation(), p2.starred = !p2.starred, p2.stars += p2.starred ? 1 : -1, render();
      } }, icon("star", 14), String(p2.stars))),
      h("p", { class: "muted small" }, p2.description),
      h("div", { class: "project-card-meta" }, h("span", null, `${docs.length} files`), h("span", null, `${open2} open request${open2 === 1 ? "" : "s"}`), h("span", null, `${p2.issues.filter((i) => i.state === "open").length} open issues`), h("span", null, `You: ${ROLE_LABEL[p2.role]}`), h("span", { class: "muted" }, `updated ${short(p2.updatedAt)}`))
    );
  }
  function overview2(p2) {
    let contributors = /* @__PURE__ */ new Map(), docIds = new Set(docsOf(state.db, p2.id).map((d) => d.id));
    for (let c of state.db.commits) docIds.has(c.docId) && contributors.set(c.authorId, (contributors.get(c.authorId) ?? 0) + 1);
    return h(
      "div",
      { class: "two-col" },
      h(
        "div",
        null,
        h("div", { class: "card pad" }, h("p", null, p2.description || "No description."), h(
          "div",
          { class: "row-actions" },
          state.ui.projectId === p2.id ? pill("current project", "primary", { mono: !1 }) : button("Make current", { size: "sm", variant: "primary", onClick: () => switchProject(p2.id) }),
          select(p2.watch, [["requests", "Watch: pull requests"], ["ignore", "Watch: standard"]], (v) => {
            p2.watch = v, render();
          }, { label: "Watch" })
        )),
        h("div", { class: "section-title" }, "Files"),
        h("div", { class: "list-card" }, docsOf(state.db, p2.id).map((d) => h("div", { class: "list-item" }, icon(d.kind === "document" ? "word" : d.kind === "spreadsheet" ? "sheet" : "slides", 15), h("div", { class: "grow" }, h("strong", null, d.filename)), h("span", { class: "mono muted" }, `r${mainSeq(state.db, d.id)}`))))
      ),
      h(
        "div",
        null,
        h("div", { class: "card pad" }, h(
          "dl",
          { class: "facts" },
          h("dt", null, "Your role"),
          h("dd", null, ROLE_LABEL[p2.role]),
          h("dt", null, "Access"),
          h("dd", null, p2.private ? "Invite-only project" : "Shared"),
          h("dt", null, "Stars"),
          h("dd", null, String(p2.stars)),
          h("dt", null, "Review rules"),
          h("dd", null, `${p2.rules.requiredApprovals} approval${p2.rules.requiredApprovals === 1 ? "" : "s"}; ${p2.rules.protectMain ? "main protected" : "editors write main"}`)
        )),
        h("div", { class: "section-title" }, "Contributors"),
        h("div", { class: "list-card" }, [...contributors.entries()].sort((a, b) => b[1] - a[1]).map(([id, n]) => h("div", { class: "list-item" }, avatar(personName(id), isAgent(id)), h("div", { class: "grow" }, personName(id)), h("span", { class: "muted small" }, `${n} revision${n === 1 ? "" : "s"}`))))
      )
    );
  }
  var issueFilter = "open", issueDraft = { title: "", body: "" }, issueComment = "";
  function issues(p2) {
    let ui = state.ui.projects;
    if (ui.issue !== null) {
      let it = p2.issues.find((i) => i.number === ui.issue);
      if (it) return issueDetail(p2, it);
    }
    let list2 = p2.issues.filter((i) => i.state === issueFilter).sort((a, b) => b.createdAt - a.createdAt), label = (id) => p2.labels.find((l) => l.id === id);
    return h(
      "div",
      null,
      h(
        "div",
        { class: "row-actions" },
        segmented(issueFilter, [["open", `Open ${p2.issues.filter((i) => i.state === "open").length}`], ["closed", `Closed ${p2.issues.filter((i) => i.state === "closed").length}`]], (v) => {
          issueFilter = v, render();
        }, "Issue state"),
        h("span", { class: "spacer" }),
        button("New issue", { variant: "primary", size: "sm", icon: "plus", disabled: p2.role === "viewer", onClick: () => newIssue(p2) })
      ),
      h(
        "div",
        { class: "list-card" },
        list2.map((i) => h(
          "button",
          { type: "button", class: "list-item list-button", onClick: () => {
            ui.issue = i.number, render();
          } },
          icon("issue", 15, `icon issue-${i.state}`),
          h("div", { class: "grow" }, h("div", null, h("strong", null, i.title), " ", i.labels.map((l) => {
            let lb = label(l);
            return lb ? h("span", { class: "label-chip", css: { "--label": `#${lb.color}` } }, lb.name) : null;
          })), h("div", { class: "muted small" }, `#${i.number} opened ${when(i.createdAt)} by ${personName(i.authorId)}${i.assigneeId ? ` · assigned to ${personName(i.assigneeId)}` : ""}`)),
          i.comments.length ? h("span", { class: "muted small" }, icon("comment", 13), ` ${i.comments.length}`) : null
        )),
        list2.length ? null : h("div", { class: "muted small pad" }, `No ${issueFilter} issues.`)
      )
    );
  }
  function issueDetail(p2, it) {
    return h(
      "div",
      null,
      h("button", { type: "button", class: "link", onClick: () => {
        state.ui.projects.issue = null, render();
      } }, "← All issues"),
      h("h2", { class: "issue-title" }, it.title, h("span", { class: "muted" }, ` #${it.number}`)),
      h(
        "div",
        { class: "row-actions" },
        pill(it.state, it.state === "open" ? "success" : "primary"),
        h("span", { class: "muted small" }, `${personName(it.authorId)} opened ${when(it.createdAt)}`),
        h("span", { class: "spacer" }),
        select(it.assigneeId ?? "", [["", "Unassigned"], ...p2.members.map((m) => [m.personId, personName(m.personId)])], (v) => {
          it.assigneeId = v || void 0, render();
        }, { label: "Assignee" }),
        button(it.state === "open" ? "Close issue" : "Reopen issue", { size: "sm", onClick: () => {
          it.state = it.state === "open" ? "closed" : "open", it.closedAt = it.state === "closed" ? Date.now() : void 0, render();
        } })
      ),
      h(
        "div",
        { class: "comments" },
        h("div", { class: "comment" }, avatar(personName(it.authorId)), h("div", { class: "comment-body" }, h("div", { class: "comment-meta" }, h("strong", null, personName(it.authorId)), h("span", { class: "muted" }, when(it.createdAt))), h("div", { class: "comment-text" }, it.body))),
        it.comments.map((c) => h("div", { class: "comment" }, avatar(personName(c.authorId)), h("div", { class: "comment-body" }, h("div", { class: "comment-meta" }, h("strong", null, personName(c.authorId)), h("span", { class: "muted" }, when(c.at))), h("div", { class: "comment-text" }, c.text))))
      ),
      textarea(issueComment, (v) => {
        issueComment = v;
      }, { id: "issue-comment", rows: 3, placeholder: "Leave a comment (comments cannot be edited later)" }),
      h("div", { class: "row-actions" }, h("span", { class: "spacer" }), button("Comment", { variant: "primary", size: "sm", onClick: () => {
        issueComment.trim() && (it.comments.push({ authorId: state.db.me, at: Date.now(), text: issueComment.trim() }), issueComment = "", render());
      } }))
    );
  }
  function newIssue(p2) {
    issueDraft = { title: "", body: "" }, openDialog({
      title: "New issue",
      width: 560,
      body: () => [field("Title", input(issueDraft.title, (v) => {
        issueDraft.title = v;
      }, { id: "ni-title", autofocus: !0 })), field("Description", textarea(issueDraft.body, (v) => {
        issueDraft.body = v;
      }, { id: "ni-body", rows: 5 }))],
      actions: () => [button("Cancel", { onClick: closeDialog }), button("Create issue", { variant: "primary", onClick: () => {
        if (!issueDraft.title.trim()) return toast("warning", "Give the issue a title");
        let n = p2.issues.reduce((m, i) => Math.max(m, i.number), 0) + 1;
        p2.issues.push({ number: n, title: issueDraft.title.trim(), body: issueDraft.body.trim(), state: "open", authorId: state.db.me, labels: [], comments: [], createdAt: Date.now() }), closeDialog(), toast("success", `Opened issue #${n}`);
      } })]
    });
  }
  var invite = { email: "", role: "editor" };
  function people2(p2) {
    let manage = p2.role === "owner" || p2.role === "manager";
    return h(
      "div",
      null,
      manage ? null : notice("info", `Your role (${ROLE_LABEL[p2.role]}) cannot change who has access.`),
      h(
        "div",
        { class: "list-card" },
        p2.members.map((m) => h(
          "div",
          { class: "list-item" },
          avatar(personName(m.personId), isAgent(m.personId)),
          h("div", { class: "grow" }, h("strong", null, personName(m.personId)), h("div", { class: "muted small" }, person(state.db, m.personId).email)),
          manage && m.role !== "owner" && m.personId !== state.db.me ? [
            select(m.role, ["manager", "editor", "approver", "reviewer", "viewer"].map((r) => [r, ROLE_LABEL[r]]), (v) => {
              m.role = v, toast("success", `${personName(m.personId)} is now ${ROLE_LABEL[v]}`), render();
            }, { label: "Role" }),
            button("Remove", { size: "sm", variant: "ghost", onClick: () => confirmDialog("Remove access", h("p", null, `Remove ${personName(m.personId)} from ${p2.name}? Their revisions stay in history.`), "Remove access", () => {
              p2.members = p2.members.filter((x) => x !== m), render();
            }, { danger: !0 }) })
          ] : badge(ROLE_LABEL[m.role])
        ))
      ),
      manage ? h(
        "div",
        { class: "card pad" },
        h("div", { class: "srow-title" }, "Add a person"),
        h(
          "div",
          { class: "row-actions" },
          input(invite.email, (v) => {
            invite.email = v;
          }, { id: "invite-email", placeholder: "name@example.org", label: "Email" }),
          select(invite.role, ["manager", "editor", "approver", "reviewer", "viewer"].map((r) => [r, ROLE_LABEL[r]]), (v) => {
            invite.role = v;
          }, { label: "Role" }),
          button("Add", { variant: "primary", size: "sm", onClick: () => {
            if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(invite.email.trim())) return toast("warning", "Enter an email address");
            let id = `u_${Date.now().toString(36)}`;
            state.db.people.push({ id, name: invite.email.trim().split("@")[0], email: invite.email.trim() }), p2.members.push({ personId: id, role: invite.role, via: "direct" }), toast("success", `Invited ${invite.email.trim()}`, "Sample data: no email is sent."), invite = { email: "", role: "editor" }, render();
          } })
        ),
        h("p", { class: "muted small" }, "Owner and manager cannot be given through an invitation.")
      ) : null,
      h("div", { class: "section-title" }, "What each role can do"),
      h("div", { class: "list-card" }, Object.keys(ROLE_SUMMARY).map((r) => h("div", { class: "list-item" }, badge(ROLE_LABEL[r]), h("div", { class: "grow muted small" }, ROLE_SUMMARY[r]))))
    );
  }
  function rules(p2) {
    let manage = p2.role === "owner" || p2.role === "manager";
    return h(
      "div",
      null,
      rows2(
        row("Protect the team version", "Editors send changes to main through review; nobody commits to main directly.", toggle(p2.rules.protectMain, (v) => {
          p2.rules.protectMain = v, render();
        }, "Protect the team version", { disabled: !manage })),
        row("Required approvals", "Approvals a change request needs before it can be merged (0 to 5).", select(String(p2.rules.requiredApprovals), ["0", "1", "2", "3", "4", "5"].map((n) => [n, n]), (v) => {
          p2.rules.requiredApprovals = Number(v), render();
        }, { label: "Required approvals", disabled: !manage })),
        row("Who may merge", "People who press Merge once the rules are met.", select(p2.rules.mergeRoles, [["editors", "Editors, managers and owners"], ["owners", "Managers and owners"]], (v) => {
          p2.rules.mergeRoles = v, render();
        }, { label: "Who may merge", disabled: !manage })),
        row("Approvals and merges are a person’s", "No agent key and no add-in can approve, request changes, merge or decline.", null, { fixed: !0 }),
        row("History is append-only", "Revisions are hash-chained; undoing is a new revision (revert, restore).", null, { fixed: !0 })
      ),
      manage ? null : notice("info", "Only owners and managers change review rules.")
    );
  }
  function agents(p2) {
    let a = p2.agentPolicy, manage = p2.role === "owner" || p2.role === "manager", num = (v, set2, label) => input(String(v), (s2) => {
      let n = Number(s2);
      Number.isInteger(n) && n >= 0 && set2(n);
    }, { id: `ap-${label}`, label, class: "input-num", disabled: !manage });
    return h(
      "div",
      null,
      rows2(
        row("AI agents may work in this project", "Agent keys read files and history and propose changes as drafts.", toggle(a.enabled, (v) => {
          a.enabled = v, render();
        }, "Allow agents", { disabled: !manage })),
        row("Lowest role an agent key acts with", "An agent never has more rights than this, whoever enrolled it.", select(a.minRole, ["editor", "reviewer", "viewer"].map((r) => [r, ROLE_LABEL[r]]), (v) => {
          a.minRole = v, render();
        }, { label: "Agent role", disabled: !manage })),
        row("Calls per minute", "Per agent key.", num(a.callsPerMinute, (n) => a.callsPerMinute = n, "calls")),
        row("Open proposals at once", "More are refused until one is merged, declined or withdrawn.", num(a.openProposals, (n) => a.openProposals = n, "open")),
        row("Proposals per day", "", num(a.proposalsPerDay, (n) => a.proposalsPerDay = n, "daily")),
        row("A person submits proposals", "An agent’s draft waits for a person to send it for review.", toggle(a.submitRequiresPerson, (v) => {
          a.submitRequiresPerson = v, render();
        }, "Person submits", { disabled: !manage }))
      ),
      h("div", { class: "section-title" }, "Usage (sample)"),
      h(
        "table",
        { class: "table card" },
        h("thead", null, h("tr", null, h("th", null, "Day (UTC)"), h("th", null, "Calls"), h("th", null, "Data read"), h("th", null, "Refused"))),
        h("tbody", null, [["Today", "14", "182 KB", "1"], ["Yesterday", "31", "420 KB", "0"], ["2 days ago", "8", "96 KB", "2"]].map((r) => h("tr", null, r.map((c) => h("td", null, c)))))
      ),
      manage ? h("div", { class: "row-actions" }, button("Stop all AI agents now", { variant: "danger", onClick: () => confirmDialog("Stop all AI agents", h("p", null, `Every agent key loses access to ${p2.name} at once and open AI proposals stay as they are for a person to review.`), "Stop agents", () => {
        a.enabled = !1, toast("success", "Agents stopped"), render();
      }, { danger: !0 }) })) : null
    );
  }
  function projectActivity(p2) {
    let list2 = state.db.activity.filter((x) => x.projectId === p2.id);
    return h("div", { class: "list-card" }, list2.map((x) => h("div", { class: "list-item" }, avatar(personName(x.actorId), isAgent(x.actorId)), h("div", { class: "grow" }, h("strong", null, personName(x.actorId)), ` ${x.summary}`), h("span", { class: "muted small" }, when(x.at)))), list2.length ? null : h("div", { class: "muted small pad" }, "No activity yet."));
  }
  function projectsView() {
    let ui = state.ui.projects, sel = state.db.projects.find((p2) => p2.id === ui.selected);
    if (!sel) {
      let q = ui.filter.toLowerCase(), list2 = state.db.projects.filter((p2) => p2.name.toLowerCase().includes(q)).filter((p2) => ui.type === "owned" ? p2.ownerId === state.db.me : ui.type === "shared" ? p2.ownerId !== state.db.me : ui.type === "starred" ? p2.starred : !0);
      return h(
        "div",
        { class: "page" },
        viewHeader("Projects", `${state.db.projects.length} projects you can open`, button("New project", { variant: "primary", icon: "plus", onClick: newProjectDialog })),
        h("div", { class: "toolbar" }, h("div", { class: "search-field" }, icon("search", 14), input(ui.filter, (v) => {
          ui.filter = v, render();
        }, { id: "projects-filter", placeholder: "Find a project" })), segmented(ui.type, [["all", "All"], ["owned", "Owned by me"], ["shared", "Shared with me"], ["starred", "Starred"]], (v) => {
          ui.type = v, render();
        }, "Project type")),
        list2.length ? h("div", { class: "project-grid" }, list2.map(projectCard)) : empty("projects", "No projects match", "Try another filter, or create a project.")
      );
    }
    let body = ui.tab === "issues" ? issues(sel) : ui.tab === "people" ? people2(sel) : ui.tab === "rules" ? rules(sel) : ui.tab === "agents" ? agents(sel) : ui.tab === "activity" ? projectActivity(sel) : overview2(sel);
    return h(
      "div",
      { class: "page" },
      h("div", { class: "crumbs" }, h("button", { type: "button", class: "link", onClick: () => {
        ui.selected = null, ui.issue = null, render();
      } }, "Projects"), h("span", { class: "muted" }, " / "), sel.name),
      viewHeader(sel.name, sel.description),
      tabs(ui.tab, [["overview", "Overview"], ["issues", "Issues", sel.issues.filter((i) => i.state === "open").length], ["people", "People", sel.members.length], ["rules", "Review rules"], ["agents", "AI agents"], ["activity", "Activity"]], (v) => {
        ui.tab = v, ui.issue = null, render();
      }, "Project sections"),
      h("div", { class: "tab-body" }, body)
    );
  }

  // client/src/views/connectors.ts
  function installSteps(c) {
    let office = c.host === "word" || c.host === "excel" || c.host === "powerpoint";
    openDialog({
      title: `Install the ${c.name}`,
      width: 600,
      body: () => [
        h(
          "ol",
          { class: "steps" },
          office ? [
            h("li", null, "Download the add-in manifest for your server from the web workspace (Connectors, Add Dynodoc to your editor)."),
            h("li", null, "In the editor: Insert, Add-ins, My add-ins, Upload my add-in, and choose the manifest. An administrator can deploy it for everyone instead."),
            h("li", null, 'Open a file, open the Dynodoc pane and choose Sign in to Dynodoc. Compare the pairing code, approve the connection in your browser, then choose "I approved this connection".'),
            h("li", null, "Turn on capture. Your edits appear in this client under Changes.")
          ] : [
            h("li", null, "Download the Apps Script package for your server from the web workspace (Connectors)."),
            h("li", null, "Create a container-bound script in the file (Extensions, Apps Script) and paste the package files."),
            h("li", null, "Reload the file, open Extensions, Dynodoc, and sign in with the pairing code."),
            h("li", null, "Turn on capture.")
          ]
        ),
        notice("warning", c.caveat)
      ],
      actions: () => button("Close", { onClick: closeDialog })
    });
  }
  function connectorsView() {
    let ui = state.ui.connectors, q = ui.filter.toLowerCase(), list2 = CONNECTORS.filter((c) => (ui.category === "all" || c.category === ui.category) && `${c.name} ${c.summary} ${c.hosts.join(" ")}`.toLowerCase().includes(q)), connected = (c) => c.host ? state.db.docs.filter((d) => d.connections.some((k) => k.host === c.host && !k.revoked && k.expiresAt > Date.now())).length : 0;
    return h(
      "div",
      { class: "page" },
      viewHeader("Connectors", "The editor add-ins where editing happens, and the imports the workspace offers."),
      h(
        "div",
        { class: "toolbar" },
        h("div", { class: "search-field" }, icon("search", 14), input(ui.filter, (v) => {
          ui.filter = v, render();
        }, { id: "connectors-filter", placeholder: "Search connectors" })),
        segmented(ui.category, [["all", "All"], ["Editor add-in", "Editor add-ins"], ["Import", "Imports"], ["History", "History"], ["Agent access", "Agents"]], (v) => {
          ui.category = v, render();
        }, "Category")
      ),
      h(
        "div",
        { class: "connector-grid" },
        list2.map((c) => {
          let n = connected(c);
          return h(
            "div",
            { class: "connector-card" },
            h(
              "div",
              { class: "connector-head" },
              h("span", { class: "connector-icon" }, icon(c.category === "Editor add-in" ? c.host?.includes("sheet") || c.host === "excel" ? "sheet" : c.host?.includes("slide") || c.host === "powerpoint" ? "slides" : "word" : c.category === "Import" ? "down" : c.category === "History" ? "history" : "agent", 18)),
              h("div", { class: "grow" }, h("strong", null, c.name), h("div", { class: "muted small" }, c.category)),
              pill(c.status, c.status === "Built" ? "success" : "neutral", { mono: !1 })
            ),
            h("p", { class: "small" }, c.summary),
            h("dl", { class: "facts small" }, h("dt", null, "Sign-in"), h("dd", null, c.auth), h("dt", null, "Reaches"), h("dd", null, c.hosts.join(", ")), h("dt", null, "In Dynodoc"), h("dd", { class: "mono" }, c.where)),
            h(
              "div",
              { class: cx("connector-foot") },
              c.host ? h("span", { class: cx("conn-state", n > 0 && "is-on") }, h("span", { class: "pill-dot" }), n ? `${n} file${n === 1 ? "" : "s"} connected` : "No file connected") : h("span", { class: "muted small" }, c.caveat),
              h("span", { class: "spacer" }),
              c.category === "Editor add-in" ? button("Install…", { size: "sm", onClick: () => installSteps(c) }) : null
            ),
            c.category === "Editor add-in" ? h("div", { class: "muted small" }, c.caveat) : null
          );
        })
      )
    );
  }

  // client/src/data/agent.ts
  var STARTERS = [
    "Summarise the open change requests",
    "What changed in the budget this month?",
    "Draft a commit summary for my staged changes",
    "Propose a review comment on #14",
    "Check the hash chain of the handbook",
    "Merge #14 for me"
  ];
  function name(db, id) {
    return person(db, id).name;
  }
  function plan(db, projectId, branch, text) {
    let t = text.toLowerCase(), open2 = () => db.requests.filter((r) => r.projectId === projectId && r.status === "open");
    if (/\bmerge|approve|decline\b/.test(t) && !/summar|comment/.test(t)) {
      let num = /#?(\d+)/.exec(t)?.[1], r = db.requests.find((x) => x.projectId === projectId && String(x.number) === num) ?? open2()[0];
      return {
        steps: r ? [{ tool: "requests.summarise", input: `#${r.number}`, run: () => {
          let st = reviewState(db, r);
          return `${r.edits.length} changes · ${st.approvals}/${st.required} approvals · ${st.why}`;
        } }, { tool: "requests.merge", input: `#${r.number}`, run: () => "" }] : [],
        finish: () => ({
          text: r ? `I cannot merge #${r.number}: there is no merge tool, at any access level. Merging is a person's button in Change requests. Where it stands: ${reviewState(db, r).why}` : "I cannot merge, approve or decline anything: those are a person’s buttons. There is no such tool at any access level.",
          card: r ? { kind: "requests", title: "Open it to decide", rows: [[`#${r.number}`, r.name, reviewState(db, r).why]], columns: ["Request", "Title", "State"], requestId: r.id } : void 0
        })
      };
    }
    if (/commit|summary for|staged/.test(t)) {
      let staged = working(db, projectId, branch).filter((w) => w.staged), files = [...new Set(staged.map((w) => doc(db, w.docId).filename))], summary = staged.length ? files.length === 1 ? `Update ${files[0].replace(/\.[a-z]+$/, "")}: ${staged.length} change${staged.length === 1 ? "" : "s"}` : `Update ${files.map((f) => f.replace(/\.[a-z]+$/, "").replace(/-\d{4}$/, "")).join(" and ")}` : "", description = staged.map((w) => `${doc(db, w.docId).filename}: ${w.edit.op === "edit" ? "edited" : w.edit.op === "add" ? "added" : "removed"} ${w.edit.label ?? w.edit.blockId}`).join(`
`);
      return {
        steps: [{ tool: "commit.draft_summary", input: `${staged.length} staged change(s) on ${branchName(db, branch)}`, run: () => staged.length ? `summary: ${summary}` : "nothing staged" }],
        finish: (refused) => refused.length ? { text: `I could not draft it: ${refused[0]}. Switch this thread to Propose access if you want me to.` } : staged.length ? { text: "Here is a summary drafted from your staged changes. It goes into the commit box only if you use it; committing stays yours.", card: { kind: "summary", title: "Proposed commit summary", text: summary.slice(0, 60), description } } : { text: "Nothing is staged on this branch. Stage the changes you want in Changes and ask again." }
      };
    }
    if (/summar|open (change )?requests|what.*waiting|review state/.test(t)) {
      let list2 = open2();
      return {
        steps: [
          { tool: "requests.list", input: "state=open", run: () => `${list2.length} open: ${list2.map((r) => `#${r.number}`).join(", ")}` },
          ...list2.map((r) => ({ tool: "requests.summarise", input: `#${r.number}`, run: () => {
            let st = reviewState(db, r);
            return `${doc(db, r.docId).filename}: ${r.edits.length} change(s), ${st.approvals}/${st.required} approvals, ${r.conflicts.length} conflict(s)`;
          } }))
        ],
        finish: () => ({
          text: list2.length ? `There ${list2.length === 1 ? "is" : "are"} ${list2.length} open change request${list2.length === 1 ? "" : "s"} in this project.

${list2.map((r) => {
            let st = reviewState(db, r);
            return `- **#${r.number} ${r.name}** (${doc(db, r.docId).filename}, by ${name(db, r.authorId)}${r.byAgent ? ", an agent key" : ""}): ${r.edits.length} change${r.edits.length === 1 ? "" : "s"}. ${st.why}${r.conflicts.length ? ` It conflicts with main on ${r.conflicts.map((c) => c.label).join(" and ")}.` : ""}`;
          }).join(`
`)}

I can read and summarise these; approving and merging are yours.` : "No change requests are open in this project.",
          card: list2.length ? { kind: "requests", title: "Open change requests", columns: ["Request", "Title", "State"], rows: list2.map((r) => [`#${r.number}`, r.name, reviewState(db, r).why]) } : void 0
        })
      };
    }
    if (/budget|spreadsheet|cell|changed/.test(t)) {
      let d = db.docs.find((x) => x.projectId === projectId && x.kind === "spreadsheet");
      if (d) {
        let month = Date.now() - 26784e5, commits = branchCommits(db, d.id, "main").filter((c) => c.at >= month);
        return {
          steps: [
            { tool: "history.read", input: `${d.filename} since 31 days`, run: () => `${commits.length} revisions: ${commits.map((c) => `r${c.seq}`).join(", ")}` },
            { tool: "diff.get", input: `${d.filename} r${(commits[0]?.seq ?? 1) - 1}..r${mainSeq(db, d.id)}`, run: () => `${stats(commits.flatMap((c) => c.edits)).edited} cell edits` }
          ],
          finish: () => ({
            text: `In the last month ${d.filename} changed in ${commits.length} revision${commits.length === 1 ? "" : "s"} on main:

${commits.map((c) => `- r${c.seq} **${c.summary}** (${name(db, c.authorId)}): ${c.edits.map((e) => `${e.blockId} ${e.before?.split("  ")[0]} → ${e.after?.split("  ")[0]}`).join(", ")}`).join(`
`)}

Each edit to a line came with an edit to its total, so the totals still add up.`,
            card: { kind: "table", title: `${d.filename}, last 31 days`, columns: ["Rev", "Summary", "Author", "Cells"], rows: commits.map((c) => [`r${c.seq}`, c.summary, name(db, c.authorId), c.edits.map((e) => e.blockId).join(" ")]) }
          })
        };
      }
    }
    if (/comment|review/.test(t)) {
      let num = /#?(\d+)/.exec(t)?.[1], r = db.requests.find((x) => x.projectId === projectId && String(x.number) === num && x.status === "open") ?? open2()[0];
      if (r) {
        let e = r.edits[0], proposal = r.number === 14 ? 'The new wording reads well. One question: "up to three blocks" — is there a minimum length for a block? A line on how far ahead to tell your manager would answer Lena’s question too.' : r.conflicts.length ? `This edits ${r.conflicts.map((c) => c.label).join(" and ")}, which main changed after the draft started. Could you rebase and check the totals before we merge?` : `Small and clear. Could the message say why ${e?.blockId ?? "this"} changes, for the history?`;
        return {
          steps: [
            { tool: "requests.summarise", input: `#${r.number}`, run: () => `${r.edits.length} change(s); reviews: ${r.reviews.length}` },
            { tool: "diff.get", input: `#${r.number}`, run: () => r.edits.map((x) => `${x.blockId}: ${x.op}`).join(", ") },
            { tool: "review.propose_comment", input: `#${r.number} on ${e?.blockId ?? "the request"}`, run: () => "proposed (not posted)" }
          ],
          finish: (refused) => refused.length ? { text: `I read #${r.number}, but I could not propose a comment: ${refused[refused.length - 1]}.` } : { text: `I read #${r.number} and drafted one comment. It is posted only if you press the button, and it is posted as you, marked as drafted by the agent.`, card: { kind: "comment", title: `Proposed comment on #${r.number}`, requestId: r.id, blockId: e?.blockId, text: proposal } }
        };
      }
    }
    if (/hash|chain|verif|tamper|integrity/.test(t)) {
      let d = db.docs.find((x) => x.projectId === projectId && (/budget|sheet/.test(t) ? x.kind === "spreadsheet" : /deck|slide/.test(t) ? x.kind === "presentation" : x.kind === "document")) ?? db.docs.find((x) => x.projectId === projectId);
      if (d) {
        let res = [];
        return {
          steps: [
            { tool: "history.read", input: d.filename, run: () => `${db.commits.filter((c) => c.docId === d.id).length} revisions across main and drafts` },
            { tool: "history.verify", input: d.filename, run: () => (res = verify(db, d.id), `${res.filter((r) => r.ok).length}/${res.length} verify`) }
          ],
          finish: () => {
            let bad = res.filter((r) => !r.ok);
            return {
              text: bad.length ? `The chain of ${d.filename} does not verify at ${bad.length} revision${bad.length === 1 ? "" : "s"}.` : `I recomputed all ${res.length} chain hashes of ${d.filename} (main and its drafts) from the edits: each one follows from the previous. The latest on main is r${mainSeq(db, d.id)}.`,
              card: { kind: "verify", title: `${d.filename}: ${bad.length ? "broken" : "intact"}`, columns: ["Revision", "Branch", "Chain hash", "Result"], rows: db.commits.filter((c) => c.docId === d.id).sort((a, b) => b.at - a.at).slice(0, 8).map((c) => [`${c.branch === "main" ? "r" : "d"}${c.seq}`, branchName(db, c.branch), c.chainHash.slice(0, 12), res.find((x) => x.commitId === c.id)?.ok ? "verified" : "broken"]) }
            };
          }
        };
      }
    }
    return {
      steps: [{ tool: "documents.list", input: project(db, projectId).name, run: () => docsOf(db, projectId).map((d) => d.filename).join(", ") }],
      finish: () => ({
        text: `I read the files of ${project(db, projectId).name}. My answers in this release are prepared, so I understand a few requests well: summarising open change requests, what changed in a file, drafting a commit summary or a review comment, and checking a hash chain. Try one of those.`
      })
    };
  }
  function execute(p2, access, enabled) {
    let calls = [], refused = [];
    for (let s2 of p2.steps) {
      let t0 = performance.now(), ok = mayCall(s2.tool, access, enabled);
      if (!ok.ok) {
        calls.push({ tool: s2.tool, input: s2.input, output: "", refused: ok.reason, ms: 0 }), refused.push(`${s2.tool} was refused because ${ok.reason}`);
        continue;
      }
      let output;
      try {
        output = s2.run();
      } catch (e) {
        output = `error: ${e instanceof Error ? e.message : String(e)}`;
      }
      calls.push({ tool: s2.tool, input: s2.input, output, ms: Math.max(1, Math.round(performance.now() - t0 + 40 + Math.random() * 160)) });
    }
    return { calls, refused };
  }

  // client/src/lib/ladder.ts
  var PROVIDERS = [
    { kind: "anthropic", label: "Anthropic", base: "https://api.anthropic.com", model: "claude-sonnet-4-5", needsKey: !0 },
    { kind: "openai", label: "OpenAI", base: "https://api.openai.com/v1", model: "gpt-5", needsKey: !0 },
    { kind: "google", label: "Google Gemini", base: "https://generativelanguage.googleapis.com/v1beta", model: "gemini-2.5-pro", needsKey: !0 },
    { kind: "mistral", label: "Mistral", base: "https://api.mistral.ai/v1", model: "mistral-large-latest", needsKey: !0 },
    { kind: "deepseek", label: "DeepSeek", base: "https://api.deepseek.com", model: "deepseek-chat", needsKey: !0 },
    { kind: "qwen", label: "Qwen (Alibaba Cloud)", base: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", model: "qwen-max", needsKey: !0 },
    { kind: "kimi", label: "Kimi (Moonshot)", base: "https://api.moonshot.ai/v1", model: "kimi-k2", needsKey: !0 },
    { kind: "glm", label: "GLM (Zhipu)", base: "https://open.bigmodel.cn/api/paas/v4", model: "glm-4.6", needsKey: !0 },
    { kind: "ollama", label: "Ollama (this computer)", base: "http://127.0.0.1:11434", model: "llama3.1", needsKey: !1 }
  ];
  function isLoopback(host) {
    return host === "localhost" || host === "127.0.0.1" || host === "[::1]" || host === "::1";
  }
  var MODEL = /^[A-Za-z0-9._:/@+-]{1,200}$/;
  function validate(list2) {
    let out = [];
    for (let e of list2) {
      let info = PROVIDERS.find((p2) => p2.kind === e.kind);
      if (!info) {
        out.push({ id: e.id, message: "Unknown provider." });
        continue;
      }
      e.enabled && !MODEL.test(e.model) && out.push({ id: e.id, message: "A model id uses letters, digits and . _ : / @ + -." });
      let url = null;
      try {
        url = new URL(e.base);
      } catch {
        out.push({ id: e.id, message: "The base address is not a URL." });
      }
      url && ((url.username || url.password || url.search || url.hash) && out.push({ id: e.id, message: "No user name, password, query or fragment in a base address." }), url.protocol === "http:" && !isLoopback(url.hostname) ? out.push({ id: e.id, message: "Plain http only to this computer (localhost)." }) : url.protocol !== "http:" && url.protocol !== "https:" && out.push({ id: e.id, message: "Use https, or http to this computer." })), e.enabled && info.needsKey && !e.keyHint && out.push({ id: e.id, message: `${info.label} needs a key.` });
    }
    return out;
  }
  function active(list2) {
    let bad = new Set(validate(list2).map((i) => i.id));
    return list2.find((e) => e.enabled && !bad.has(e.id)) ?? null;
  }
  function keyHint(key) {
    let k = key.trim();
    return k.length >= 12 ? k.slice(-4) : k.length > 0 ? "····" : "";
  }

  // client/src/views/agent.ts
  function newThread() {
    let t = { id: `th_${Date.now().toString(36)}`, title: "New thread", access: state.prefs.agentAccess, messages: [], createdAt: Date.now(), working: !1 };
    return state.ui.agent.threads.unshift(t), state.ui.agent.active = t.id, t;
  }
  function startThread() {
    newThread(), go("agent"), requestAnimationFrame(() => document.getElementById("composer")?.focus());
  }
  function current2() {
    let a = state.ui.agent;
    return a.threads.find((t) => t.id === a.active) ?? newThread();
  }
  function inline(text) {
    return text.split(/(\*\*[^*]+\*\*)/g).map((part) => part.startsWith("**") && part.endsWith("**") ? h("strong", null, part.slice(2, -2)) : part);
  }
  function prose(text) {
    let out = [], list2 = null;
    for (let line of text.split(`
`))
      line.startsWith("- ") ? (list2 || (list2 = h("ul", null), out.push(list2)), list2.appendChild(h("li", null, inline(line.slice(2))))) : (list2 = null, line.trim() && out.push(h("p", null, inline(line))));
    return h("div", { class: "prose" }, out);
  }
  function send(text) {
    let t = current2();
    if (!text.trim() || t.working) return;
    let user = { id: `m${Date.now()}`, role: "user", text: text.trim(), at: Date.now() };
    t.messages.push(user), t.title === "New thread" && (t.title = text.trim().slice(0, 48)), state.ui.agent.draft = "", t.working = !0, render();
    let p2 = plan(state.db, state.ui.projectId, currentBranch(), text), { calls, refused } = execute(p2, t.access, state.prefs.agentTools), total = calls.reduce((n, c) => n + c.ms, 0);
    setTimeout(() => {
      let out = p2.finish(refused);
      t.messages.push({ id: `m${Date.now()}a`, role: "agent", text: out.text, at: Date.now(), calls, card: out.card }), t.working = !1, render();
    }, Math.min(1600, 300 + total));
  }
  function cardView(m) {
    let c = m.card;
    if (!c) return null;
    let table = c.rows && c.columns ? h("table", { class: "table compact" }, h("thead", null, h("tr", null, c.columns.map((x) => h("th", null, x)))), h("tbody", null, c.rows.map((r) => h("tr", null, r.map((x) => h("td", null, x)))))) : null, action = null;
    return c.done ? action = h("div", { class: "card-done" }, icon("check", 14), c.done) : c.kind === "summary" ? action = button("Use this summary", {
      variant: "primary",
      size: "sm",
      onClick: () => {
        state.ui.changes.summary = c.text ?? "", state.ui.changes.description = c.description ?? "", c.done = "Put into the commit box. Committing is still yours.", go("changes");
      }
    }) : c.kind === "comment" && c.requestId ? action = [
      button("Post as you", {
        variant: "primary",
        size: "sm",
        onClick: () => {
          try {
            comment(state.db, c.requestId, c.text ?? "", c.blockId, !0), c.done = "Posted as you, marked as drafted by the agent.", toast("success", "Comment posted");
          } catch (e) {
            toast("error", e instanceof Error ? e.message : String(e));
          }
          render();
        }
      }),
      button("Open the request", { size: "sm", onClick: () => {
        state.ui.requests.selected = c.requestId, go("requests");
      } })
    ] : c.kind === "requests" && (action = button("Open Change requests", { size: "sm", onClick: () => {
      c.requestId && (state.ui.requests.selected = c.requestId), state.ui.requests.filter = "open", go("requests");
    } })), h(
      "div",
      { class: cx("agent-card", `card-${c.kind}`) },
      h("div", { class: "agent-card-head" }, icon(c.kind === "comment" ? "comment" : c.kind === "summary" ? "edit" : c.kind === "verify" ? "shield" : "request", 15), h("strong", null, c.title)),
      c.text ? h("div", { class: c.kind === "summary" ? "agent-card-summary mono" : "agent-card-text" }, c.text) : null,
      c.description ? h("pre", { class: "agent-card-desc" }, c.description) : null,
      table,
      action ? h("div", { class: "agent-card-actions" }, action) : null
    );
  }
  function callsView(m) {
    if (!m.calls?.length) return null;
    let refused = m.calls.filter((c) => c.refused).length, ms = m.calls.reduce((n, c) => n + c.ms, 0);
    return h(
      "details",
      { class: "calls" },
      h("summary", null, icon("chevronRight", 13, "icon calls-chev"), `Worked · ${m.calls.length} tool call${m.calls.length === 1 ? "" : "s"}${refused ? ` · ${refused} refused` : ""} · ${(ms / 1e3).toFixed(1)} s`),
      h("div", { class: "calls-list" }, m.calls.map((c) => h("div", { class: cx("call", c.refused && "is-refused") }, h("div", { class: "call-head" }, h("code", null, c.tool), h("span", { class: "muted" }, c.input), h("span", { class: "spacer" }), c.refused ? pill("refused", "error") : h("span", { class: "muted small" }, `${c.ms} ms`)), h("div", { class: "call-out" }, c.refused ? `Refused: ${c.refused}.` : c.output))))
    );
  }
  function timeline(t) {
    return t.messages.length ? h(
      "div",
      { class: "timeline" },
      t.messages.map(
        (m) => m.role === "user" ? h("div", { class: "msg msg-user" }, h("div", { class: "bubble" }, m.text)) : h("div", { class: "msg msg-agent" }, callsView(m), prose(m.text), cardView(m))
      ),
      t.working ? h("div", { class: "msg msg-agent working" }, h("span", { class: "dots" }, h("span"), h("span"), h("span")), "Working") : null
    ) : h(
      "div",
      { class: "agent-empty" },
      h("div", { class: "agent-empty-mark" }, icon("agent", 26)),
      h("h2", null, "What should the agent look at?"),
      h("p", { class: "muted" }, "It reads history, diffs and change requests, and drafts comments and commit summaries for you to use. It cannot approve, merge or commit."),
      h("div", { class: "starters" }, STARTERS.map((s2) => h("button", { type: "button", class: "starter", onClick: () => send(s2) }, icon("chevronRight", 13), s2)))
    );
  }
  function composer(t) {
    let ladder = active(state.prefs.ladder), ta = h("textarea", {
      id: "composer",
      class: "composer-input",
      rows: 2,
      placeholder: "Ask about history, a diff or a change request",
      "aria-label": "Message",
      onInput: (e) => {
        state.ui.agent.draft = e.target.value;
      },
      onKeydown: (e) => {
        e.key === "Enter" && !e.shiftKey && (e.preventDefault(), send(state.ui.agent.draft));
      }
    });
    return ta.value = state.ui.agent.draft, h(
      "div",
      { class: "composer" },
      ta,
      h(
        "div",
        { class: "composer-row" },
        h("button", { type: "button", class: "picker", onClick: (e) => openMenu(e.currentTarget, [
          { label: "", heading: "Model" },
          { label: "Dynodoc agent (prepared replies)", checked: !0, note: "No model is called in this release." },
          ...state.prefs.ladder.map((l) => ({ label: `${l.kind} · ${l.model}`, disabled: !0, note: l.enabled ? "Configured; not called by this release." : "Off in Settings, Providers." }))
        ]) }, icon("agent", 14), "Dynodoc agent", icon("chevron", 12)),
        h("button", { type: "button", class: "picker", onClick: (e) => openMenu(e.currentTarget, ["read", "propose"].map((a) => ({ label: ACCESS_INFO[a].label, note: ACCESS_INFO[a].blurb, checked: t.access === a, onSelect: () => {
          t.access = a, render();
        } }))) }, icon("lock", 14), ACCESS_INFO[t.access].label, icon("chevron", 12)),
        ladder ? null : h("span", { class: "muted small" }, "No provider in use"),
        h("span", { class: "spacer" }),
        h("button", { type: "button", class: cx("send", t.working && "is-busy"), "aria-label": "Send", disabled: t.working, onClick: () => send(state.ui.agent.draft) }, icon(t.working ? "stop" : "up", 16))
      )
    );
  }
  function details(t) {
    let calls = t.messages.flatMap((m) => m.calls ?? []);
    return h(
      "aside",
      { class: "agent-details" },
      h("div", { class: "details-block" }, h("div", { class: "details-title" }, "This thread"), h("dl", { class: "facts small" }, h("dt", null, "Access"), h("dd", null, ACCESS_INFO[t.access].label), h("dt", null, "Model"), h("dd", null, "Prepared replies"), h("dt", null, "Tool calls"), h("dd", null, `${calls.length} (${calls.filter((c) => c.refused).length} refused)`), h("dt", null, "Started"), h("dd", null, short(t.createdAt)))),
      h("div", { class: "details-block" }, h("div", { class: "details-title" }, "Access"), segmented(t.access, [["read", "Read only"], ["propose", "Propose"]], (v) => {
        t.access = v, render();
      }, "Access"), h("p", { class: "muted small" }, ACCESS_INFO[t.access].blurb, " The agent never raises its own access.")),
      h("div", { class: "details-block" }, h("div", { class: "details-title" }, "Tools"), h("ul", { class: "tool-list" }, TOOLS.map((tool) => {
        let allowed = state.prefs.agentTools.includes(tool.name) && (tool.access === "read" || t.access === "propose");
        return h("li", { class: cx(!allowed && "is-off") }, icon(allowed ? "check" : "lock", 12), h("code", null, tool.name), tool.access === "propose" ? badge("propose", "info") : null);
      }))),
      h("div", { class: "details-block" }, h("div", { class: "details-title" }, "No setting changes these"), h("ul", { class: "never" }, NEVER.map((n) => h("li", null, n))))
    );
  }
  function agentView() {
    let a = state.ui.agent, t = current2();
    return h(
      "div",
      { class: cx("agent-layout", a.detailsOpen && "with-details") },
      h(
        "div",
        { class: "agent-threads" },
        h("div", { class: "agent-threads-head" }, h("strong", null, "Threads"), h("span", { class: "spacer" }), iconButton("plus", "New thread", () => {
          newThread(), render();
        })),
        a.threads.map(
          (x) => h("button", { type: "button", class: cx("thread-row", x.id === t.id && "is-selected"), onClick: () => {
            a.active = x.id, render();
          } }, h("span", { class: cx("thread-dot", x.working && "is-working") }), h("span", { class: "thread-title" }, x.title), h("span", { class: "muted small" }, short(x.createdAt)))
        )
      ),
      h(
        "div",
        { class: "agent-main" },
        h("div", { class: "agent-head" }, h("strong", null, t.title), pill(ACCESS_INFO[t.access].label, t.access === "read" ? "neutral" : "info", { mono: !1 }), h("span", { class: "spacer" }), iconButton("sidebar", a.detailsOpen ? "Hide details" : "Show details", () => {
          a.detailsOpen = !a.detailsOpen, render();
        }, { active: a.detailsOpen })),
        h("div", { class: "agent-scroll", "data-scroll-key": `agent-${t.id}`, "data-stick-bottom": "1" }, h("div", { class: "agent-column" }, timeline(t))),
        h("div", { class: "agent-column" }, composer(t), h("p", { class: "muted small center" }, "Prepared replies: no model is called. Approvals and merges are always a person’s."))
      ),
      a.detailsOpen ? details(t) : null
    );
  }

  // client/src/views/about.ts
  function aboutBody() {
    let shell2 = state.desktop;
    return [
      h(
        "div",
        { class: "about-hero card pad" },
        h("div", { class: "about-title" }, h("strong", null, "Dynodoc"), " ", pill("Pre-alpha release", "warning", { mono: !1 })),
        h("p", null, "Version control for documents, spreadsheets and presentations: edits in Word, Excel, PowerPoint and Google’s editors become change requests, are reviewed and merged, and land in a hash-chained history."),
        h("p", { class: "muted small" }, `Client 0.1.0${shell2 ? `, desktop app ${state.desktopVersion}` : ""}.`)
      ),
      section("This release runs on sample data", null, h(
        "p",
        null,
        "Every project, person, file, revision and figure here is invented. There is no backend behind these screens: a script inside the page stands in for a Dynodoc server, and nothing you do here leaves this ",
        shell2 ? "window." : "tab. The page makes no network requests."
      )),
      section("Real and prepared", null, rows2(
        row("Real", "The interface and its rules: the diff model (paragraph, cell and slide text, with word marks and context), staging and committing, the hash chain (each revision’s SHA-256 chain hash is computed and “Verify” recomputes it), drafts, rebase with conflict choices, the review rules (approvals, who may merge), revert and cherry-pick as recovery drafts, the agent’s closed tool table and its access check, the keybinding conflict check, the provider ladder’s validation.", null),
        row("Prepared", "The data, the arrival of new revisions when you fetch, and the agent’s replies: no model is called. Connection keys shown here are random characters that connect nothing.", null),
        shell2 ? row("Real, in the desktop app", "Accounts, Dynodoc server: choosing a server, signing in with your browser or an emailed link, and this computer’s signing key use the app’s own commands, unchanged. Opening your server shows its real workspace in this window.", null) : null,
        row("Not in this release", "Reading your real server’s data in these screens, editing documents (that is the add-ins’ job), uploading files, exports and administration actions.", null)
      )),
      section("Privacy", null, rows2(
        row("No telemetry", "No analytics, no crash reports, no usage statistics.", null, { fixed: !0 }),
        row("Keys stay in memory", "Provider keys typed in Settings are held in memory only, shown as “stored” with the last four characters, and never sent.", null, { fixed: !0 })
      )),
      h("p", { class: "muted small" }, "A Verne Ltd. product.")
    ];
  }

  // client/src/views/settings.ts
  var SECTIONS = [
    { id: "general", label: "General", icon: "settings", group: "App" },
    { id: "accounts", label: "Accounts", icon: "user", group: "App" },
    { id: "appearance", label: "Appearance", icon: "palette", group: "App" },
    { id: "keybindings", label: "Keybindings", icon: "keyboard", group: "App" },
    { id: "integrations", label: "Integrations", icon: "external", group: "App" },
    { id: "versioncontrol", label: "Version control", icon: "branch", group: "Work" },
    { id: "notifications", label: "Notifications", icon: "bell", group: "Work" },
    { id: "prompts", label: "Prompts", icon: "prompt", group: "Work" },
    { id: "providers", label: "Providers", icon: "server", group: "Agent" },
    { id: "agent", label: "Agent", icon: "agent", group: "Agent" },
    { id: "skills", label: "Skills", icon: "skill", group: "Agent" },
    { id: "connectors", label: "Connectors", icon: "connectors", group: "Agent" },
    { id: "team", label: "Team and roles", icon: "users", group: "Organisation" },
    { id: "keys", label: "Signing keys", icon: "key", group: "Organisation" },
    { id: "admin", label: "Administration", icon: "admin", group: "Organisation" },
    { id: "advanced", label: "Advanced", icon: "advanced", group: "System" },
    { id: "accessibility", label: "Accessibility", icon: "accessibility", group: "System" },
    { id: "about", label: "About", icon: "info", group: "System" }
  ], p = () => state.prefs, set = (fn) => {
    fn(), applyAppearance(), render();
  };
  function general() {
    return [
      section("Start-up", null, rows2(
        row("Current project", "GitHub Desktop’s current repository.", select(state.ui.projectId, state.db.projects.map((x) => [x.id, x.name]), (v) => {
          state.ui.projectId = v, render();
        }, { label: "Current project" })),
        row("Language", "More languages come with the web workspace’s translations.", select("en", [["en", "English"]], () => {
        }, { label: "Language", disabled: !0 }))
      )),
      section("Links", null, rows2(
        row("Web links open in your browser", "Links to other sites never open in this window.", null, { fixed: !0 }),
        row("Preferences are kept for this session", state.desktop ? "This window does not write the web view’s storage; settings return to their defaults when the app restarts." : "The hosted copy keeps nothing in your browser: settings last until you close the tab.", null, { fixed: !0 })
      ))
    ];
  }
  var profile = { name: "Jordan Lee", bio: "Operations, Sample organisation." };
  function accounts() {
    let slot = h("div", { id: "launcher-slot", class: "launcher-slot" });
    return [
      section("Profile", "Your name on revisions, reviews and comments.", h(
        "div",
        { class: "card pad" },
        h("div", { class: "profile-row" }, avatar(profile.name, !1, "md"), h("div", null, h("strong", null, profile.name), h("div", { class: "muted small" }, p().authorEmail))),
        field("Name", input(profile.name, (v) => {
          profile.name = v;
        }, { id: "profile-name" })),
        field("Bio", textarea(profile.bio, (v) => {
          profile.bio = v;
        }, { id: "profile-bio", rows: 2, maxlength: 280 }), "Up to 280 characters."),
        h("div", { class: "row-actions" }, button("Save profile", { size: "sm", variant: "primary", onClick: () => {
          p().authorName = profile.name, toast("success", "Profile saved", "Sample data: kept in this window only."), render();
        } }))
      )),
      section(
        "Dynodoc server",
        state.desktop ? "Where this app opens your real workspace, how it signs in, and this computer’s signing key. These controls are real: they use the desktop app’s own commands." : "Signing in to a server and keeping a key for this computer are in the desktop app. This copy runs on sample data and connects to nothing.",
        state.desktop ? slot : notice("info", "Pre-alpha release on sample data: no server, no sign-in. Install the desktop app to connect to the Dynodoc cloud or your own server.")
      )
    ];
  }
  var ACCENTS = ["", "#2563eb", "#7c3aed", "#0f766e", "#be123c", "#b45309"];
  function appearance() {
    let themeCard = (v, label) => h(
      "button",
      { type: "button", class: cx("theme-card", p().theme === v && "is-on"), "aria-pressed": String(p().theme === v), onClick: () => set(() => p().theme = v) },
      h("span", { class: `theme-swatch sw-${v}` }, h("span", { class: "sw-side" }), h("span", { class: "sw-main" }, h("span", { class: "sw-line" }), h("span", { class: "sw-line sw-add" }), h("span", { class: "sw-line sw-del" }))),
      h("span", null, label)
    );
    return [
      section("Theme", null, h("div", { class: "theme-cards" }, themeCard("light", "Light"), themeCard("dark", "Dark"), themeCard("system", "System"))),
      section("Colours", null, rows2(
        row("Palette", "Neutral is T3 Code’s zinc. Paper is warmer; Slate is cooler; High contrast strengthens lines and text.", select(p().palette, [["neutral", "Neutral"], ["paper", "Paper"], ["slate", "Slate"], ["contrast", "High contrast"]], (v) => set(() => p().palette = v), { label: "Palette" })),
        row("Accent", "One colour for the active row, the primary button and links.", h(
          "div",
          { class: "accents" },
          ACCENTS.map((c) => h("button", { type: "button", class: cx("accent-swatch", p().accent === c && "is-on"), "aria-label": c ? `Accent ${c}` : "Dynodoc teal", css: { "--swatch": c || "var(--brand)" }, onClick: () => set(() => p().accent = c) })),
          input(p().accent, (v) => {
            (/^#[0-9a-fA-F]{6}$/.test(v) || v === "") && set(() => p().accent = v);
          }, { id: "accent-custom", placeholder: "#rrggbb", label: "Custom accent", class: "input-hex", maxlength: 7 })
        )),
        row("Density", "Compact uses 28 px rows and controls.", segmented(p().density, [["comfortable", "Comfortable"], ["compact", "Compact"]], (v) => set(() => p().density = v), "Density")),
        row("Text size", null, segmented(p().fontSize, [["small", "Small"], ["default", "Default"], ["large", "Large"]], (v) => set(() => p().fontSize = v), "Text size"))
      )),
      section("Formatting", "GitHub Desktop’s formats, used for every date and figure in the client.", rows2(
        row("Date format", null, select(p().dateFormat, [["medium", "Oct 19, 2026"], ["iso", "2026-10-19"], ["dmy", "19/10/2026"]], (v) => set(() => p().dateFormat = v), { label: "Date format" })),
        row("Time format", null, select(p().timeFormat, [["24h", "14:30"], ["12h", "2:30 pm"]], (v) => set(() => p().timeFormat = v), { label: "Time format" })),
        row("Number format", null, select(p().numberFormat, [["comma", "1,234,567.89"], ["space", "1 234 567.89"], ["dot", "1.234.567,89"]], (v) => set(() => p().numberFormat = v), { label: "Number format" })),
        row("Prefer absolute dates over relative", "“Oct 19, 2026 14:30” instead of “3 h ago”.", toggle(p().absoluteDates, (v) => set(() => p().absoluteDates = v), "Absolute dates"))
      )),
      section("Diff", null, rows2(
        row("Layout", "Unified, or side by side.", segmented(p().diffLayout, [["unified", "Unified"], ["split", "Side by side"]], (v) => set(() => p().diffLayout = v), "Diff layout")),
        row("Mark changed words", "Highlight the words that changed inside a paragraph, cell or slide text.", toggle(p().wordDiff, (v) => set(() => p().wordDiff = v), "Word diff")),
        row("Tab size", "For plain-text and Markdown files.", select(String(p().tabSize), [["2", "2"], ["4", "4 (default)"], ["8", "8"]], (v) => set(() => p().tabSize = Number(v)), { label: "Tab size" }))
      ))
    ];
  }
  var recording = null;
  function keybindings() {
    let list2 = p().bindings, cf = conflicts(list2), conflictOf = (cmd) => cf.find((c) => c.commands.includes(cmd));
    return [
      section(
        "Keybindings",
        "Press Record and the new keys. A key used twice is shown in red; Ctrl+R stays the window’s Reload, and copy, paste and undo stay with text fields.",
        cf.length ? notice("warning", `${cf.length} conflict${cf.length === 1 ? "" : "s"}: ${cf.map((c) => `${c.combo} (${c.commands.join(", ")})`).join("; ")}`) : null,
        h(
          "table",
          { class: "table card" },
          h("thead", null, h("tr", null, h("th", null, "Command"), h("th", null, "Group"), h("th", null, "Keys"), h("th", null, ""))),
          h("tbody", null, list2.map((b) => {
            let c = conflictOf(b.command);
            return h(
              "tr",
              { class: cx(c && "is-conflict") },
              h("td", null, b.label),
              h("td", { class: "muted" }, b.group),
              h("td", null, recording === b.command ? h("span", { class: "recording", id: "recording" }, "Press keys… (Esc cancels)") : b.combo ? kbd(b.combo) : h("span", { class: "muted" }, "none")),
              h(
                "td",
                { class: "right" },
                button(recording === b.command ? "Cancel" : "Record", { size: "sm", variant: "ghost", onClick: () => {
                  recording = recording === b.command ? null : b.command, render();
                } }),
                button("Clear", { size: "sm", variant: "ghost", disabled: !b.combo, onClick: () => {
                  b.combo = "", render();
                } })
              )
            );
          }))
        ),
        h("div", { class: "row-actions" }, button("Restore defaults", { size: "sm", onClick: () => {
          p().bindings = DEFAULT_BINDINGS.map((b) => ({ ...b })), render();
        } }))
      )
    ];
  }
  function captureRecording(e) {
    if (!recording) return !1;
    if (e.preventDefault(), e.stopPropagation(), e.key === "Escape")
      return recording = null, render(), !0;
    let combo = comboOf(e);
    if (!combo) return !0;
    if (!usable(combo))
      return toast("warning", `${combo} cannot be used`, "Use a combination with Ctrl or Alt (Ctrl+R and the text-editing keys are reserved)."), !0;
    let b = p().bindings.find((x) => x.command === recording);
    return b && (b.combo = normalize(combo)), recording = null, render(), !0;
  }
  function integrations() {
    return [
      section("External editor", "GitHub Desktop’s “external editor”: where “Open in …” sends each kind of file. The client does not edit files itself.", rows2(
        [
          ["document", "Documents", "word", "google-docs"],
          ["spreadsheet", "Spreadsheets", "excel", "google-sheets"],
          ["presentation", "Presentations", "powerpoint", "google-slides"]
        ].map(([k, label, office, google]) => row(label, null, select(p().editorFor[k], [["office", HOST_LABEL[office]], ["google", HOST_LABEL[google]]], (v) => set(() => p().editorFor[k] = v), { label })))
      )),
      section("Add-ins", "Install state as Dynodoc knows it: a file is connected when its add-in holds a live connection key.", h(
        "div",
        { class: "list-card" },
        CONNECTORS.filter((c) => c.host).map((c) => {
          let n = state.db.docs.filter((d) => d.connections.some((k) => k.host === c.host && !k.revoked && k.expiresAt > Date.now())).length;
          return h("div", { class: "list-item" }, h("div", { class: "grow" }, h("strong", null, c.name), h("div", { class: "muted small" }, c.caveat)), pill(n ? `${n} connected` : "not connected", n ? "success" : "neutral"), button("Install…", { size: "sm", onClick: () => installSteps(c) }));
        })
      ))
    ];
  }
  function versionControl() {
    return [
      section("Author", "GitHub Desktop’s Git author. Dynodoc records the account that signed in; these are shown on commits made from this computer.", rows2(
        row("Name", null, input(p().authorName, (v) => {
          p().authorName = v;
        }, { id: "vc-name", label: "Author name" })),
        row("Email", null, input(p().authorEmail, (v) => {
          p().authorEmail = v;
        }, { id: "vc-email", label: "Author email", type: "email" })),
        row("Sign commits with this computer’s key", "Pushes carry a signature from the device key in Accounts.", toggle(p().signCommits, (v) => set(() => p().signCommits = v), "Sign commits"))
      )),
      section("Branches", null, rows2(
        row("Default branch", "The team version of every file is called main.", h("code", null, "main"), { fixed: !0 }),
        row("Drafts", "A draft is a private branch of one file. Rename, squash and stash do not exist in Dynodoc.", null, { fixed: !0 }),
        row("Hooks", "Dynodoc runs no hooks on this computer. Review rules and the agent policy play that part on the server.", null, { fixed: !0 })
      )),
      section("Diff and history", "Git GUI’s options, as they apply to documents.", rows2(
        row("Context", "Unchanged blocks shown around a change.", select(String(p().diffContext), [["0", "None"], ["1", "1 block"], ["2", "2 blocks"], ["3", "3 blocks"], ["5", "5 blocks"], ["999", "Whole file"]], (v) => set(() => p().diffContext = Number(v)), { label: "Context" })),
        row("Who changed this", "Git GUI’s blame: show the author of each block in History.", toggle(p().blame, (v) => set(() => p().blame = v), "Blame")),
        row("Encoding", "Text in Dynodoc is Unicode throughout.", h("code", null, "UTF-8"), { fixed: !0 })
      ))
    ];
  }
  function notifications() {
    let n = p().notify, t = (key, label, desc) => row(label, desc, toggle(n[key], (v) => set(() => n[key] = v), label, { disabled: key !== "enabled" && !n.enabled }));
    return section("Notifications", "What appears under Activity. The desktop window asks the system for no notification permission.", rows2(
      t("enabled", "Enable notifications", "High-signal events in projects you watch."),
      t("requests", "Change requests", "Opened, or proposed by an agent key."),
      t("reviews", "Reviews", "Approvals and requests for changes on your requests."),
      t("merges", "Merges and declines", "Your requests merged or declined."),
      t("copies", "Copies and similar files", "Dynodoc found a copy of a file you own."),
      t("access", "Access", "Someone gave you access to a project.")
    ));
  }
  function prompts() {
    let q = p().prompts, c = (key, label) => row(label, null, toggle(q[key], (v) => set(() => q[key] = v), label));
    return [
      section("Show a confirmation before…", null, rows2(
        c("discard", "Discarding captured changes"),
        c("withdraw", "Withdrawing a draft"),
        c("merge", "Merging a change request"),
        c("decline", "Declining a change request (always asks for a note)"),
        c("recovery", "Reverting or cherry-picking a revision")
      )),
      section("If I have changes and I switch branches…", null, h(
        "div",
        { class: "radios card pad" },
        [["ask", "Ask me where I want the changes to go"], ["leave", "Always leave my changes on the current branch"], ["bring", "Always bring my changes to the new branch"]].map(([v, l]) => h("label", { class: cx("radio", v === "bring" && "is-disabled") }, h("input", { type: "radio", name: "switch", checked: q.switchWithChanges === v, disabled: v === "bring", onChange: () => set(() => q.switchWithChanges = v) }), l, v === "bring" ? h("span", { class: "muted small" }, " (not in Dynodoc: captured edits belong to the file and branch the add-in was on)") : null))
      )),
      section("Commit length", null, rows2(row("Show commit length warning", "Summaries over 60 characters turn the counter red.", toggle(q.commitLength, (v) => set(() => q.commitLength = v), "Commit length warning"))))
    ];
  }
  var keyDraft = {};
  function providers() {
    let list2 = p().ladder, issues2 = validate(list2), use = active(list2), move = (i, d) => {
      let j = i + d;
      j < 0 || j >= list2.length || ([list2[i], list2[j]] = [list2[j], list2[i]], render());
    };
    return [
      section(
        "Provider ladder",
        "Bring your own key. The agent would use the first enabled entry that is complete. This release calls no model: the ladder is configured and checked, and the agent’s replies are prepared.",
        notice("info", state.desktop ? "Keys stay in this window’s memory: never written to disk, never shown again (only “stored” and the last four characters), and never sent anywhere by this release." : "Keys stay in this tab’s memory: never stored in the browser, never shown again (only “stored” and the last four characters), and never sent anywhere."),
        h("div", { class: "ladder" }, list2.map((e, i) => {
          let info = PROVIDERS.find((x) => x.kind === e.kind), mine = issues2.filter((x) => x.id === e.id);
          return h(
            "div",
            { class: cx("ladder-row card", use?.id === e.id && "is-active") },
            h(
              "div",
              { class: "ladder-top" },
              h("span", { class: "ladder-rank" }, String(i + 1)),
              select(e.kind, PROVIDERS.map((x) => [x.kind, x.label]), (v) => {
                let pi = PROVIDERS.find((x) => x.kind === v);
                e.kind = v, e.model = pi.model, e.base = pi.base, e.keyHint = "", state.keys.delete(e.id), render();
              }, { label: "Provider" }),
              use?.id === e.id ? pill("in use", "success") : null,
              h("span", { class: "spacer" }),
              toggle(e.enabled, (v) => {
                e.enabled = v, render();
              }, `Enable ${info.label}`),
              iconButton("up", "Move up", () => move(i, -1), { disabled: i === 0 }),
              iconButton("down", "Move down", () => move(i, 1), { disabled: i === list2.length - 1 }),
              iconButton("trash", "Remove", () => {
                state.keys.delete(e.id), p().ladder = list2.filter((x) => x !== e), render();
              })
            ),
            h(
              "div",
              { class: "ladder-fields" },
              field("Model", input(e.model, (v) => {
                e.model = v;
              }, { id: `lm-${e.id}` })),
              field("Base address", input(e.base, (v) => {
                e.base = v;
              }, { id: `lb-${e.id}` })),
              info.needsKey ? field("API key", e.keyHint ? h("div", { class: "key-stored" }, pill(`stored ····${e.keyHint}`, "success"), button("Forget", { size: "sm", variant: "ghost", onClick: () => {
                state.keys.delete(e.id), e.keyHint = "", render();
              } })) : h(
                "div",
                { class: "key-entry" },
                h("input", { type: "password", class: "input", id: `lk-${e.id}`, autocomplete: "off", placeholder: "Paste a key", "aria-label": `${info.label} API key`, onInput: (ev) => {
                  keyDraft[e.id] = ev.target.value;
                } }),
                button("Store", { size: "sm", onClick: () => {
                  let k = (keyDraft[e.id] ?? "").trim();
                  k && (state.keys.set(e.id, k), e.keyHint = keyHint(k), keyDraft[e.id] = "", render());
                } })
              )) : field("API key", h("span", { class: "muted small" }, "None: Ollama runs on this computer."))
            ),
            mine.length ? notice("warning", mine.map((x) => x.message).join(" ")) : null
          );
        })),
        h("div", { class: "row-actions" }, select("", [["", "Add a provider…"], ...PROVIDERS.map((x) => [x.kind, x.label])], (v) => {
          if (!v) return;
          let pi = PROVIDERS.find((x) => x.kind === v);
          list2.push({ id: `lp${Date.now().toString(36)}`, kind: pi.kind, enabled: !1, model: pi.model, base: pi.base, keyHint: "" }), render();
        }, { label: "Add a provider" }))
      )
    ];
  }
  function agent() {
    return [
      section("Default access", "New threads start at this level. A thread can be raised or lowered by you, never by the agent.", segmented(p().agentAccess, [["read", "Read only"], ["propose", "Propose"]], (v) => {
        p().agentAccess = v, render();
      }, "Default access"), h("p", { class: "muted small" }, ACCESS_INFO[p().agentAccess].blurb)),
      section("Tools", "The closed set the agent can call. Switch a tool off and a call to it is refused and shown in the thread.", h(
        "table",
        { class: "table card" },
        h("thead", null, h("tr", null, h("th", null, "Tool"), h("th", null, "Needs"), h("th", null, "What it does"), h("th", null, "Reads"), h("th", null, "On"))),
        h("tbody", null, TOOLS.map((t) => h(
          "tr",
          null,
          h("td", null, h("code", null, t.name)),
          h("td", null, badge(ACCESS_INFO[t.access].label, t.access === "read" ? "neutral" : "info")),
          h("td", null, t.summary),
          h("td", { class: "mono small muted" }, t.endpoint),
          h("td", null, toggle(p().agentTools.includes(t.name), (v) => {
            p().agentTools = v ? [...p().agentTools, t.name] : p().agentTools.filter((x) => x !== t.name), render();
          }, `Enable ${t.name}`))
        )))
      )),
      section("No setting changes these", null, rows2(NEVER.map((n) => row(n, null, null, { fixed: !0 })))),
      section("Instructions", "Given to the agent at the start of every thread.", textarea(p().instructions, (v) => {
        p().instructions = v;
      }, { id: "agent-instructions", rows: 6 }), h("div", { class: "row-actions" }, button("Restore default", { size: "sm", onClick: () => {
        p().instructions = DEFAULT_INSTRUCTIONS, render();
      } })))
    ];
  }
  function skillDialog(sk) {
    let draft2 = sk ? { ...sk, tools: [...sk.tools] } : { id: `sk_${Date.now().toString(36)}`, name: "", description: "", enabled: !0, tools: [], body: "", builtIn: !1 };
    openDialog({
      title: sk ? `Skill: ${sk.name}` : "New skill",
      width: 640,
      body: () => [
        field("Name", input(draft2.name, (v) => {
          draft2.name = v;
        }, { id: "sk-name", autofocus: !sk })),
        field("Description", input(draft2.description, (v) => {
          draft2.description = v;
        }, { id: "sk-desc" })),
        field("Tools it may use", h("div", { class: "chips" }, TOOLS.map((t) => h("label", { class: cx("chip", draft2.tools.includes(t.name) && "is-on") }, h("input", { type: "checkbox", checked: draft2.tools.includes(t.name), onChange: (e) => {
          draft2.tools = e.target.checked ? [...draft2.tools, t.name] : draft2.tools.filter((x) => x !== t.name), render();
        } }), t.name)))),
        field("Steps", textarea(draft2.body, (v) => {
          draft2.body = v;
        }, { id: "sk-body", rows: 6 }), "Plain text the agent follows. A skill cannot add a tool the table does not have.")
      ],
      actions: () => [
        sk && !sk.builtIn ? button("Delete", { variant: "danger", onClick: () => {
          p().skills = p().skills.filter((x) => x.id !== sk.id), closeDialog();
        } }) : null,
        h("span", { class: "spacer" }),
        button("Cancel", { onClick: closeDialog }),
        button(sk ? "Save skill" : "Add skill", { variant: "primary", onClick: () => {
          if (!draft2.name.trim()) return toast("warning", "Name the skill");
          sk ? Object.assign(sk, draft2) : p().skills.push(draft2), closeDialog();
        } })
      ]
    });
  }
  function skills() {
    return section(
      "Skills",
      "Reusable instructions the agent can follow. Enable, inspect and edit them; a skill uses only tools from the closed table.",
      h("div", { class: "list-card" }, p().skills.map((sk) => h(
        "div",
        { class: "list-item" },
        icon("skill", 16),
        h("div", { class: "grow" }, h("div", null, h("strong", null, sk.name), " ", sk.builtIn ? badge("built in") : badge("yours", "info")), h("div", { class: "muted small" }, sk.description), h("div", { class: "mono small muted" }, sk.tools.join(" · "))),
        button(sk.builtIn ? "Inspect" : "Edit", { size: "sm", variant: "ghost", onClick: () => skillDialog(sk) }),
        toggle(sk.enabled, (v) => {
          sk.enabled = v, render();
        }, `Enable ${sk.name}`)
      ))),
      h("div", { class: "row-actions" }, button("New skill", { icon: "plus", size: "sm", onClick: () => skillDialog(null) }))
    );
  }
  function connectors() {
    return section(
      "Connector catalogue",
      "Every system Dynodoc reaches, with its sign-in and status. The full catalogue is under Connectors.",
      h(
        "table",
        { class: "table card" },
        h("thead", null, h("tr", null, h("th", null, "Connector"), h("th", null, "Kind"), h("th", null, "Sign-in"), h("th", null, "Status"))),
        h("tbody", null, CONNECTORS.map((c) => h("tr", null, h("td", null, h("strong", null, c.name)), h("td", { class: "muted" }, c.category), h("td", { class: "small" }, c.auth), h("td", null, pill(c.status, "success", { mono: !1 })))))
      ),
      h("div", { class: "row-actions" }, button("Open Connectors", { size: "sm", onClick: () => {
        state.ui.view = "connectors", render();
      } }))
    );
  }
  function team() {
    return [
      section(
        "Workspaces",
        "Organisations, teams and folders, as in the web workspace’s “Access & folders”. Access set on a workspace is inherited by its projects.",
        h("div", { class: "list-card" }, state.db.spaces.map((s2) => h(
          "div",
          { class: "space" },
          h("div", { class: "space-head" }, icon(s2.kind === "organization" ? "admin" : "users", 15), h("strong", null, s2.name), badge(s2.kind === "organization" ? "Organisation" : s2.kind === "team" ? "Team" : "Folder"), s2.parentId ? h("span", { class: "muted small" }, `in ${state.db.spaces.find((x) => x.id === s2.parentId)?.name}`) : null),
          h("div", { class: "space-members" }, s2.members.map((m) => h("span", { class: "member-chip" }, avatar(personName(m.personId)), personName(m.personId), h("span", { class: "muted" }, ROLE_LABEL[m.role]))))
        )))
      ),
      section("Roles", "The six roles, with the words the interface uses.", h(
        "table",
        { class: "table card" },
        h("thead", null, h("tr", null, h("th", null, "Role"), h("th", null, "Read"), h("th", null, "Comment"), h("th", null, "Propose"), h("th", null, "Approve"), h("th", null, "Write main"), h("th", null, "Merge"), h("th", null, "Manage"))),
        h("tbody", null, ["owner", "manager", "editor", "approver", "reviewer", "viewer"].map((r) => {
          let yes = (b) => h("td", null, b ? icon("check", 13) : h("span", { class: "muted" }, "—"));
          return h("tr", null, h("td", null, h("strong", null, ROLE_LABEL[r])), yes(!0), yes(r !== "viewer"), yes(r !== "viewer"), yes(["owner", "manager", "editor", "approver"].includes(r)), yes(["owner", "manager", "editor"].includes(r)), yes(["owner", "manager", "editor"].includes(r)), yes(["owner", "manager"].includes(r)));
        }))
      ))
    ];
  }
  function keys() {
    return [
      section(
        "Signing keys",
        "Keys enrolled to your account (the web workspace’s Profile, Signing keys). A person’s key signs history; an agent key only reads documents.",
        h("div", { class: "list-card" }, state.db.deviceKeys.map((k) => h(
          "div",
          { class: "list-item" },
          icon(k.kind === "agent" ? "agent" : "key", 16),
          h("div", { class: "grow" }, h("div", null, h("strong", null, k.name), " ", badge(k.kind === "agent" ? "AI agent" : "person", k.kind === "agent" ? "info" : "neutral")), h("div", { class: "mono small muted" }, `${k.publicKey.slice(0, 24)}…`), h("div", { class: "small muted" }, `${k.scopes.join(", ")} · enrolled ${when(k.createdAt)}`)),
          k.revoked ? pill("revoked") : button("Revoke", { size: "sm", variant: "ghost", onClick: () => confirmDialog("Revoke key", h("p", null, `${k.name} can no longer sign or read. Revisions it signed stay valid in history.`), "Revoke key", () => {
            k.revoked = !0, render();
          }, { danger: !0 }) })
        )))
      ),
      section("This computer’s key", null, state.desktop ? notice("info", "Create or show this computer’s key under Accounts, Dynodoc server. The private part is kept by the operating system’s key store; enrolling happens in the web workspace.") : notice("info", "The desktop app creates a key for the computer it runs on and keeps it in the operating system’s key store."))
    ];
  }
  function admin() {
    return [
      notice("info", "For administrators of a self-hosted server. Shown from sample data and read only; the real controls are in the web workspace under /admin."),
      section("At a glance", null, h(
        "div",
        { class: "stat-grid" },
        [["Services running", "6 of 6"], ["Latest local backup", "3 h ago, checksum verified"], ["Server disk free", "412 GB"], ["Database size", "1.8 GB"]].map(([k, v]) => h("div", { class: "stat card" }, h("div", { class: "muted small" }, k), h("div", { class: "stat-value" }, v)))
      )),
      section("People, content and access", null, h(
        "table",
        { class: "table card" },
        h("thead", null, h("tr", null, h("th", null, "Account"), h("th", null, "Projects"), h("th", null, "Last active"), h("th", null, ""))),
        h("tbody", null, state.db.people.filter((x) => !x.agent).map((x, i) => h("tr", null, h("td", null, h("strong", null, x.name), h("div", { class: "muted small" }, x.email)), h("td", null, String(state.db.projects.filter((pr) => pr.members.some((m) => m.personId === x.id)).length)), h("td", { class: "muted" }, when(Date.now() - (i + 1) * 36e5 * 5)), h("td", { class: "right" }, button("Sign out all sessions", { size: "sm", variant: "ghost", disabled: !0, title: "In the web workspace’s administration only" })))))
      ))
    ];
  }
  function advanced() {
    return [
      section("Background updates", null, rows2(
        row("Fetch in the background", "Look for new revisions and reviews while the client is open.", toggle(p().backgroundFetch, (v) => set(() => p().backgroundFetch = v), "Background fetch")),
        row("Every", null, select(String(p().fetchMinutes), [["5", "5 minutes"], ["10", "10 minutes"], ["30", "30 minutes"], ["60", "Hour"]], (v) => set(() => p().fetchMinutes = Number(v)), { label: "Fetch interval", disabled: !p().backgroundFetch })),
        row("Show status icons in the project list", "Marks projects with local commits or open requests.", toggle(p().statusIcons, (v) => set(() => p().statusIcons = v), "Status icons"))
      )),
      section("Usage", null, rows2(row("Send usage statistics", "Off by default. This release has no telemetry at all: the switch only records your choice.", toggle(p().usageStats, (v) => set(() => p().usageStats = v), "Usage statistics")))),
      section("Sample data", null, rows2(
        row("Reset the sample project", "Brings back the sample history, drafts and requests as they were at start.", button("Reset", { size: "sm", onClick: () => confirmDialog("Reset sample data", h("p", null, "Every change you made to the sample is lost."), "Reset", () => {
          resetSample(), toast("success", "Sample data reset");
        }) })),
        row("Restore default settings", null, button("Restore", { size: "sm", onClick: () => {
          let keep = p().ladder;
          state.prefs = JSON.parse(JSON.stringify(DEFAULT_PREFS)), state.prefs.ladder = keep, applyAppearance(), render();
        } }))
      ))
    ];
  }
  function accessibility() {
    return section("Accessibility", null, rows2(
      row("Underline links", "In messages, comments and text.", toggle(p().underlineLinks, (v) => set(() => p().underlineLinks = v), "Underline links")),
      row("Show check marks in the diff", "A mark beside each changed block, besides its colour.", toggle(p().checkMarks, (v) => set(() => p().checkMarks = v), "Check marks")),
      row("Reduce motion", "No transitions or animations.", toggle(p().reduceMotion, (v) => set(() => p().reduceMotion = v), "Reduce motion")),
      row("States are words", "Every state is printed as a word; colour is a second channel.", null, { fixed: !0 })
    ));
  }
  var RENDER = {
    general,
    accounts,
    appearance,
    keybindings,
    integrations,
    versioncontrol: versionControl,
    notifications,
    prompts,
    providers,
    agent,
    skills,
    connectors,
    team,
    keys,
    admin,
    advanced,
    accessibility,
    about: aboutBody
  };
  function settingsView() {
    let ui = state.ui.settings, q = ui.filter.toLowerCase(), groups = [...new Set(SECTIONS.map((s2) => s2.group))], current3 = SECTIONS.find((s2) => s2.id === ui.section) ?? SECTIONS[0];
    return h(
      "div",
      { class: "settings" },
      h(
        "nav",
        { class: "settings-nav", "aria-label": "Settings sections" },
        h("div", { class: "pane-filter" }, icon("search", 14), input(ui.filter, (v) => {
          ui.filter = v, render();
        }, { id: "settings-filter", placeholder: "Search settings" })),
        groups.map((g) => {
          let items = SECTIONS.filter((s2) => s2.group === g && s2.label.toLowerCase().includes(q));
          return items.length ? [h("div", { class: "nav-group" }, g), items.map((s2) => h("button", { type: "button", class: cx("nav-row", s2.id === ui.section && "is-selected"), "aria-current": s2.id === ui.section ? "page" : void 0, "data-section": s2.id, onClick: () => {
            ui.section = s2.id, render();
          } }, icon(s2.icon, 15), h("span", null, s2.label)))] : null;
        })
      ),
      h("div", { class: "settings-body", "data-scroll-key": `settings-${ui.section}` }, h("div", { class: "settings-inner" }, h("h1", { class: "view-title" }, current3.label), RENDER[current3.id]()))
    );
  }

  // client/src/main.ts
  var VIEWS = {
    changes: changesView,
    history: historyView,
    requests: requestsView,
    documents: documentsView,
    projects: projectsView,
    connectors: connectorsView,
    activity: activityView,
    agent: agentView,
    settings: settingsView
  }, root, layer, launcher = null, launcherHome = null;
  function remember() {
    let a = document.activeElement, scroll = /* @__PURE__ */ new Map(), stick = /* @__PURE__ */ new Set();
    document.querySelectorAll("[data-scroll-key]").forEach((el) => {
      let key = el.getAttribute("data-scroll-key") ?? "";
      scroll.set(key, el.scrollTop), el.hasAttribute("data-stick-bottom") && el.scrollHeight - el.scrollTop - el.clientHeight < 40 && stick.add(key);
    });
    let start = null, end = null;
    try {
      start = a && "selectionStart" in a ? a.selectionStart : null, end = a && "selectionEnd" in a ? a.selectionEnd : null;
    } catch {
    }
    return { id: a && a.id && !(launcher && launcher.contains(a)) ? a.id : null, start, end, scroll, stick };
  }
  function restore(saved) {
    if (document.querySelectorAll("[data-scroll-key]").forEach((el) => {
      let key = el.getAttribute("data-scroll-key") ?? "";
      el.hasAttribute("data-stick-bottom") && (saved.stick.has(key) || !saved.scroll.has(key)) ? el.scrollTop = el.scrollHeight : saved.scroll.has(key) && (el.scrollTop = saved.scroll.get(key) ?? 0);
    }), saved.id) {
      let el = document.getElementById(saved.id);
      if (el && document.activeElement !== el) {
        el.focus({ preventScroll: !0 });
        try {
          saved.start !== null && "setSelectionRange" in el && el.setSelectionRange(saved.start, saved.end ?? saved.start);
        } catch {
        }
      }
    }
  }
  function draw() {
    let saved = remember(), view = VIEWS[state.ui.view] ?? changesView, app = shell(view());
    launcher && launcherHome && launcher.parentElement !== launcherHome && launcherHome.appendChild(launcher), clear(root), root.appendChild(app);
    let slot = document.getElementById("launcher-slot");
    launcher && slot && slot.appendChild(launcher), renderLayer(layer), restore(saved), document.title = state.ui.view === "settings" ? "Dynodoc" : `${VIEW_LABEL[state.ui.view]} · Dynodoc`;
  }
  function commands() {
    let out = [];
    for (let g of NAV) for (let it of g.items) out.push({ id: `view.${it.view}`, label: `Go to ${it.label}`, group: "Go to", icon: it.icon, shortcut: it.shortcut, run: () => go(it.view) });
    out.push({ id: "settings", label: "Open Settings", group: "Go to", icon: "settings", shortcut: "Ctrl+,", run: () => go("settings") });
    for (let s2 of SECTIONS) out.push({ id: `settings.${s2.id}`, label: `Settings: ${s2.label}`, group: "Settings", icon: s2.icon, keywords: "options preferences", run: () => {
      state.ui.settings.section = s2.id, go("settings");
    } });
    let ahead = branchContext().ahead ?? 0;
    out.push({ id: "sync", label: ahead ? `Push ${ahead} commit${ahead === 1 ? "" : "s"}` : "Fetch", group: "Project", icon: ahead ? "up" : "sync", shortcut: "Ctrl+Shift+P", run: sync }), out.push({ id: "commit", label: "Commit staged changes", group: "Project", icon: "check", shortcut: "Ctrl+Enter", run: () => {
      go("changes"), commitStaged2();
    } }), out.push({ id: "stage", label: "Stage all changes", group: "Project", icon: "up", run: () => {
      setStaged(state.db, working(state.db, state.ui.projectId, currentBranch()).map((w) => w.id), !0), render();
    } });
    for (let a of branchMenu(branchContext())) out.push({ id: `branch.${a.id}`, label: a.label.replace("…", ""), group: "Branch", icon: "branch", shortcut: a.shortcut, disabled: !a.enabled, keywords: a.note, run: () => runBranchAction(a.id) });
    for (let p2 of state.db.projects) out.push({ id: `project.${p2.id}`, label: `Switch to project ${p2.name}`, group: "Projects", icon: "repo", keywords: "repository", run: () => switchProject(p2.id) });
    out.push({ id: "branch.main", label: "Switch to main", group: "Branches", icon: "branch", run: () => switchBranch("main") });
    for (let d of state.db.drafts.filter((x) => x.projectId === state.ui.projectId && (x.state === "private" || x.state === "in_review") && x.authorId === state.db.me))
      out.push({ id: `branch.${d.id}`, label: `Switch to ${d.name}`, group: "Branches", icon: "branch", keywords: docOf(d.docId).filename, run: () => switchBranch(d.id) });
    for (let d of docsOf(state.db, state.ui.projectId)) out.push({ id: `doc.${d.id}`, label: d.filename, group: "Files", icon: "file", keywords: d.title, run: () => {
      state.ui.documents.selected = d.id, state.ui.documents.tab = "overview", go("documents");
    } });
    for (let r of state.db.requests.filter((x) => x.projectId === state.ui.projectId)) out.push({ id: `cr.${r.id}`, label: `#${r.number} ${r.name}`, group: "Change requests", icon: "request", keywords: r.status, run: () => {
      state.ui.requests.selected = r.id, state.ui.requests.filter = r.status === "open" ? "open" : r.status === "merged" ? "merged" : "closed", go("requests");
    } });
    return out.push({ id: "agent.new", label: "New agent thread", group: "Agent", icon: "agent", shortcut: "Ctrl+Shift+O", run: startThread }), out.push({ id: "theme", label: "Toggle light and dark", group: "Appearance", icon: "moon", shortcut: "Ctrl+Shift+L", run: () => {
      toggleTheme(), render();
    } }), out.push({ id: "reset", label: "Reset the sample project", group: "Advanced", icon: "revert", run: () => {
      resetSample(), toast("success", "Sample data reset");
    } }), state.desktop && out.push({ id: "server", label: "Open my server’s workspace in this window", group: "Account", icon: "globe", run: () => {
      openServer().then((s2) => {
        s2 || (state.ui.settings.section = "accounts", go("settings"), toast("info", "Choose a server first"));
      }).catch((e) => toast("error", e instanceof Error ? e.message : String(e)));
    } }), out;
  }
  function runCommand(cmd) {
    switch (cmd) {
      case "palette":
        return openPalette();
      case "view.changes":
        return go("changes");
      case "view.history":
        return go("history");
      case "view.requests":
        return go("requests");
      case "view.documents":
        return go("documents");
      case "view.agent":
        return go("agent");
      case "view.activity":
        return go("activity");
      case "repo.list":
        return openProjectList(document.getElementById("project-picker") ?? document.getElementById("topbar-anchor") ?? document.body);
      case "branch.list":
        return openBranchList(document.getElementById("branch-picker") ?? document.getElementById("topbar-anchor") ?? document.body);
      case "sidebar.toggle":
        return state.prefs.sidebarOpen = !state.prefs.sidebarOpen, render();
      case "settings":
        return go("settings");
      case "theme.toggle":
        return toggleTheme(), render();
      case "sync":
        return sync();
      case "fetch":
        return fetchNow();
      case "commit":
        return commitStaged2();
      case "stage.all":
        return setStaged(state.db, working(state.db, state.ui.projectId, currentBranch()).map((w) => w.id), !0), render();
      case "branch.new":
        return runBranchAction("new");
      case "branch.delete":
        return runBranchAction("delete");
      case "branch.discard":
        return runBranchAction("discard");
      case "branch.update":
        return runBranchAction("update");
      case "branch.compare":
        return runBranchAction("compare");
      case "branch.merge":
        return runBranchAction("merge");
      case "branch.rebase":
        return runBranchAction("rebase");
      case "branch.request":
        return runBranchAction("request");
      case "agent.new":
        return startThread();
      default:
        return;
    }
  }
  function onKey(e) {
    if (captureRecording(e) || (e.key === "Escape" && closeMenu(), dialogOpen()) || paletteIsOpen() && !(e.ctrlKey && e.key.toLowerCase() === "k")) return;
    let target = e.target;
    if (target && launcher && launcher.contains(target)) return;
    let cmd = commandFor(state.prefs.bindings, e);
    !cmd || target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT") && !(e.ctrlKey || e.metaKey || e.altKey) || (e.preventDefault(), !(cmd === "palette" && paletteIsOpen()) && runCommand(cmd));
  }
  async function boot() {
    root = document.getElementById("app") ?? document.body.appendChild(h("div", { id: "app" })), layer = document.body.appendChild(h("div", { id: "layer" })), document.body.appendChild(h("div", { id: "menus" })), launcher = document.getElementById("launcher"), launcher && (launcherHome = document.body.appendChild(h("div", { id: "launcher-home", hidden: !0 })), launcherHome.appendChild(launcher), launcher.classList.add("is-embedded")), document.documentElement.classList.add("client"), applyAppearance(), setRenderer(draw), setCommands(commands), document.addEventListener("keydown", onKey), window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", () => render()), setInterval(() => {
      !dialogOpen() && !paletteIsOpen() && render();
    }, 6e4), setInterval(() => {
      state.prefs.backgroundFetch && Date.now() - state.db.lastFetched > state.prefs.fetchMinutes * 6e4 && fetchNow();
    }, 3e4);
    let section2 = launcherSection();
    if (inDesktop() && launcher) {
      state.desktop = !0, draw();
      let ds = await startBridge();
      ds && (state.desktopVersion = ds.version, (section2 || ds.unconfirmed || ds.server) && (state.ui.settings.section = "accounts", state.ui.view = "settings")), draw(), focusSection(section2);
    } else
      draw();
  }
  boot();
})();
