const LANG_BY_EXT = {
    rs: 'rust',
    js: 'javascript',
    mjs: 'javascript',
    cjs: 'javascript',
    ts: 'typescript',
    tsx: 'tsx',
    py: 'python',
    pyi: 'python',
};

const SUPPORTED_LANGS = new Set(['rust', 'javascript', 'typescript', 'tsx', 'python']);

const SCOPE_TO_CSS = {
    keyword: 'tok-keyword',
    'keyword-type': 'tok-keyword-type',
    'keyword-function': 'tok-keyword-func',
    'keyword-return': 'tok-keyword',
    'keyword-import': 'tok-keyword-import',
    'keyword-export': 'tok-keyword-import',
    'keyword-exception': 'tok-keyword',
    'keyword-modifier': 'tok-keyword-mod',
    string: 'tok-string',
    number: 'tok-number',
    comment: 'tok-comment',
    function: 'tok-function',
    'function-method': 'tok-function',
    'function-builtin': 'tok-function',
    type: 'tok-type',
    'type-builtin': 'tok-type',
    variable: 'tok-variable',
    'variable-param': 'tok-variable',
    'variable-builtin': 'tok-variable',
    constant: 'tok-constant',
    'constant-builtin': 'tok-constant',
    boolean: 'tok-constant',
    operator: 'tok-operator',
    punctuation: 'tok-punct',
    'punct-bracket': 'tok-punct',
    'punct-delim': 'tok-punct',
    tag: 'tok-tag',
    'tag-builtin': 'tok-tag',
    attribute: 'tok-attr',
    module: 'tok-module',
    label: 'tok-label',
    markup: 'tok-markup',
};

function escapeHtml(s) {
    return s
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}

function spansToHtml(spans, text) {
    if (!spans || spans.length === 0) {
        return escapeHtml(text);
    }
    const sorted = spans.slice().sort((a, b) => a.s - b.s || a.e - b.e);
    let out = '';
    let cursor = 0;
    for (const span of sorted) {
        if (span.s < cursor || span.s >= span.e) continue;
        if (span.s > cursor) {
            out += escapeHtml(text.slice(cursor, span.s));
        }
        const cls = SCOPE_TO_CSS[span.scope] || 'tok-default';
        out += '<span class="' + cls + '">' + escapeHtml(text.slice(span.s, span.e)) + '</span>';
        cursor = span.e;
    }
    if (cursor < text.length) {
        out += escapeHtml(text.slice(cursor));
    }
    return out;
}

let worker = null;
let nextId = 1;
const pending = new Map();
let workerReady = null;
let workerError = null;

function ensureWorker() {
    if (worker) return workerReady;
    try {
        worker = new Worker(new URL('./syntax-worker.js', import.meta.url), {
            type: 'module',
        });
    } catch (e) {
        workerError = e;
        throw e;
    }
    worker.onmessage = (e) => {
        const { id, ok } = e.data;
        const resolver = pending.get(id);
        if (!resolver) return;
        pending.delete(id);
        if (ok) resolver.resolve(e.data);
        else resolver.reject(new Error(e.data.error || 'highlight failed'));
    };
    worker.onerror = (e) => {
        const message = e.message || String(e);
        workerError = new Error(message);
        for (const resolver of pending.values()) {
            resolver.reject(workerError);
        }
        pending.clear();
    };
    workerReady = Promise.resolve();
    return workerReady;
}

function postToWorker(payload) {
    ensureWorker();
    const id = nextId++;
    return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        worker.postMessage({ id, ...payload });
    });
}

function detectLanguage(path) {
    if (!path) return null;
    const slash = path.lastIndexOf('/');
    const basename = slash >= 0 ? path.slice(slash + 1) : path;
    const dot = basename.lastIndexOf('.');
    if (dot < 0) return null;
    const ext = basename.slice(dot + 1).toLowerCase();
    const lang = LANG_BY_EXT[ext];
    return lang && SUPPORTED_LANGS.has(lang) ? lang : null;
}

function isSupported(langId) {
    return langId != null && SUPPORTED_LANGS.has(langId);
}

async function highlight(langId, text, isNewFile) {
    if (!isSupported(langId) || !text) {
        return { html: escapeHtml(text || ''), spans: [], parseMs: 0, capMs: 0, totalMs: 0 };
    }
    try {
        const res = await postToWorker({ langId, text, newFile: !!isNewFile });
        const html = spansToHtml(res.spans, text);
        return {
            html,
            spans: res.spans,
            parseMs: res.parseMs || 0,
            capMs: res.capMs || 0,
            totalMs: res.totalMs || 0,
        };
    } catch (e) {
        console.warn('[syntax] highlight failed:', e);
        return { html: escapeHtml(text), spans: [], parseMs: 0, capMs: 0, totalMs: 0 };
    }
}

async function init() {
    try {
        await ensureWorker();
        console.log('[syntax] worker ready');
    } catch (e) {
        console.warn('[syntax] worker failed to start, highlighting disabled:', e);
    }
}

window.syntax = { detectLanguage, isSupported, highlight, init };

init();
