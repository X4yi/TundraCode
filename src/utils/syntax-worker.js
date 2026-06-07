import { Parser, Query, Language } from '../vendor/syntax/tree-sitter.js';

const VENDOR_BASE = new URL('../vendor/syntax/', import.meta.url).href;

const LANG_FILES = {
    rust:       'tree-sitter-rust.wasm',
    javascript: 'tree-sitter-javascript.wasm',
    typescript: 'tree-sitter-typescript.wasm',
    tsx:        'tree-sitter-tsx.wasm',
    python:     'tree-sitter-python.wasm',
};

const QUERY_FILES = {
    rust:       'queries/rust.scm',
    javascript: 'queries/javascript.scm',
    typescript: 'queries/typescript.scm',
    tsx:        'queries/tsx.scm',
    python:     'queries/python.scm',
};

const cache = new Map();
let initPromise = null;

function ensureRuntime() {
    if (!initPromise) {
        initPromise = Parser.init({
            locateFile: (file) => VENDOR_BASE + file,
        });
    }
    return initPromise;
}

async function loadLanguage(langId) {
    if (cache.has(langId)) return cache.get(langId);

    const [langBytes, queryText] = await Promise.all([
        fetch(VENDOR_BASE + LANG_FILES[langId]).then((r) => {
            if (!r.ok) throw new Error('failed to load grammar: ' + langId);
            return r.arrayBuffer();
        }),
        fetch(VENDOR_BASE + QUERY_FILES[langId]).then((r) => r.text()),
    ]);

    const language = await Language.load(new Uint8Array(langBytes));
    const query = new Query(language, queryText);
    const parser = new Parser();
    parser.setLanguage(language);

    const entry = { language, query, parser, tree: null };
    cache.set(langId, entry);
    return entry;
}

function byteToCharOffset(text, byteOffset) {
    let i = 0;
    let bytes = 0;
    while (i < text.length && bytes < byteOffset) {
        const code = text.charCodeAt(i);
        if (code < 0x80) bytes += 1;
        else if (code < 0x800) bytes += 2;
        else if (code >= 0xd800 && code <= 0xdbff) {
            bytes += 4;
            i += 1;
        } else bytes += 3;
        i += 1;
    }
    return i;
}

function byteRangeToCharRange(text, startByte, endByte) {
    return [byteToCharOffset(text, startByte), byteToCharOffset(text, endByte)];
}

function captureToScope(captureName) {
    const SPECIAL_MAP = {
        'keyword.type': 'keyword-type',
        'keyword.function': 'keyword-function',
        'keyword.return': 'keyword-return',
        'keyword.import': 'keyword-import',
        'keyword.export': 'keyword-export',
        'keyword.exception': 'keyword-exception',
        'keyword.modifier': 'keyword-modifier',
        'keyword.async': 'keyword-modifier',
        'keyword.storage': 'keyword-modifier',
        'function.method': 'function-method',
        'function.builtin': 'function-builtin',
        'variable.parameter': 'variable-param',
        'variable.builtin': 'variable-builtin',
        'constant.builtin': 'constant-builtin',
        'type.builtin': 'type-builtin',
        'punctuation.bracket': 'punct-bracket',
        'punctuation.delimiter': 'punct-delim',
        'tag.builtin': 'tag-builtin',
    };
    return SPECIAL_MAP[captureName] || captureName.split('.')[0];
}

const PRIORITY = {
    'keyword': 10, 'keyword-type': 10, 'keyword-function': 10,
    'keyword-return': 10, 'keyword-import': 10, 'keyword-export': 10,
    'keyword-exception': 10, 'keyword-modifier': 10,
    'string': 10, 'comment': 10, 'number': 10,
    'boolean': 10, 'operator': 10,
    'function': 8, 'function-method': 8, 'function-builtin': 8,
    'type': 8, 'type-builtin': 8,
    'constant': 7, 'constant-builtin': 7,
    'variable': 5, 'variable-param': 6, 'variable-builtin': 6,
    'module': 8, 'tag': 9, 'tag-builtin': 9,
    'attribute': 8, 'punctuation': 6, 'punct-bracket': 5, 'punct-delim': 5,
    'label': 7, 'markup': 9,
};

function collectCaptures(query, rootNode) {
    const captures = query.captures(rootNode);
    const byRange = new Map();
    for (const { name, node } of captures) {
        const scope = captureToScope(name);
        const key = node.startIndex + ':' + node.endIndex;
        const priority = PRIORITY[scope] ?? 5;
        const existing = byRange.get(key);
        if (!existing || priority > existing.priority) {
            byRange.set(key, { scope, priority, start: node.startIndex, end: node.endIndex });
        }
    }
    const out = [];
    for (const val of byRange.values()) {
        out.push({ start: val.start, end: val.end, scope: val.scope });
    }
    return out;
}

self.onmessage = async (e) => {
    const { id, langId, text, edit, newFile } = e.data;
    try {
        const entry = await loadLanguage(langId);

        if (newFile) {
            entry.tree = null;
        }

        if (edit && entry.tree) {
            entry.tree.edit(edit);
        }

        const parseStart = performance.now();
        const tree = entry.parser.parse(text, entry.tree);
        const parseMs = performance.now() - parseStart;

        const capStart = performance.now();
        const captures = collectCaptures(entry.query, tree.rootNode);
        const capMs = performance.now() - capStart;

        entry.tree = tree;

        const spans = captures.map((c) => {
            const [s, en] = byteRangeToCharRange(text, c.start, c.end);
            return { s, e: en, scope: c.scope };
        });

        self.postMessage({
            id,
            ok: true,
            spans,
            parseMs,
            capMs,
            totalMs: parseMs + capMs,
        });
    } catch (err) {
        self.postMessage({
            id,
            ok: false,
            error: String((err && err.message) || err),
        });
    }
};

try {
    await ensureRuntime();
} catch (e) {
    console.error('[syntax-worker] Runtime init failed:', e);
}
