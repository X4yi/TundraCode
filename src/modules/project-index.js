var projectSymbols = { files: {}, langExtensions: {} };
var SYMBOL_REGEX = {
    rust: {
        fn: /(?:pub\s+)?(?:unsafe\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        struct: /(?:pub\s+)?struct\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        enum: /(?:pub\s+)?enum\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        trait: /(?:pub\s+)?trait\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        impl: /impl\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        mod: /(?:pub\s+)?mod\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        type: /(?:pub\s+)?type\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        const: /(?:pub\s+)?const\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*:/g,
    },
    javascript: {
        fn: /(?:function\s+|async\s+function\s+)([a-zA-Z_$][a-zA-Z0-9_$]*)/g,
        class: /class\s+([a-zA-Z_$][a-zA-Z0-9_$]*)/g,
        const: /(?:export\s+)?const\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*[=:]/g,
        let: /(?:export\s+)?let\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*[=:]/g,
        var: /(?:export\s+)?var\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*[=:]/g,
        arrow: /(?:const|let|var)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)\s*=\s*(?:async\s*)?\(/g,
        export: /export\s+(?:default\s+)?(?:function|class|const|let|var)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)/g,
    },
    python: {
        fn: /def\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(/g,
        class: /class\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        async_fn: /async\s+def\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(/g,
    },
    cpp: {
        fn: /(?:virtual\s+|static\s+)?(?:inline\s+)?(?:const\s+)?(?:[\w:]+[\s*&]+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*(?:const|override|final|volatile|&)?\s*(?:{|;)/g,
        class: /class\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        struct: /struct\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        enum: /enum\s+(?:class\s+)?([a-zA-Z_][a-zA-Z0-9_]*)/g,
        namespace: /namespace\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        template: /template\s*<[^>]*>\s*(?:class|struct|auto)\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
    },
    c: {
        fn: /(?:static\s+|inline\s+)?(?:[\w*\s]+)?([a-zA-Z_][a-zA-Z0-9_]*)\s*\([^)]*\)\s*(?:{|;)/g,
        struct: /struct\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        enum: /enum\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
        typedef: /typedef\s+.*\s+([a-zA-Z_][a-zA-Z0-9_]*);/g,
        define: /#define\s+([a-zA-Z_][a-zA-Z0-9_]*)/g,
    },
};

var LANG_BY_EXT_FOR_INDEX = {
    '.rs': 'rust',
    '.js': 'javascript',
    '.jsx': 'javascript',
    '.ts': 'javascript',
    '.tsx': 'javascript',
    '.py': 'python',
    '.cpp': 'cpp',
    '.cc': 'cpp',
    '.cxx': 'cpp',
    '.hpp': 'cpp',
    '.hh': 'cpp',
    '.h': 'c',
    '.c': 'c',
};

function langFromPath(path) {
    var ext = '';
    var dotIdx = path.lastIndexOf('.');
    if (dotIdx >= 0) ext = path.substring(dotIdx).toLowerCase();
    return LANG_BY_EXT_FOR_INDEX[ext] || null;
}

function extractSymbols(lang, content) {
    var symbols = [];
    var regexes = SYMBOL_REGEX[lang];
    if (!regexes) return symbols;

    var lines = content.split('\n');
    for (var kind in regexes) {
        if (!regexes.hasOwnProperty(kind)) continue;
        regexes[kind].lastIndex = 0;
        var match;
        while ((match = regexes[kind].exec(content)) !== null) {
            var lineNum = 1;
            for (var i = 0; i < match.index; i++) {
                if (content[i] === '\n') lineNum++;
            }
            symbols.push({
                name: match[1],
                kind: kind,
                line: lineNum,
                col: match.index - content.lastIndexOf('\n', match.index) - 1,
            });
        }
    }
    return symbols;
}

function indexFile(path, content) {
    var lang = langFromPath(path);
    if (!lang) return;
    var symbols = extractSymbols(lang, content);
    projectSymbols.files[path] = { lang: lang, symbols: symbols, size: content.length };
}

function removeFileIndex(path) {
    delete projectSymbols.files[path];
}

function buildProjectSummary() {
    var parts = [];
    var fileCount = 0;
    var symbolCount = 0;

    for (var path in projectSymbols.files) {
        if (!projectSymbols.files.hasOwnProperty(path)) continue;
        var info = projectSymbols.files[path];
        fileCount++;
        if (info.symbols.length > 0) {
            var names = info.symbols.map(function(s) { return s.name + ' (' + s.kind + ')'; }).join(', ');
            parts.push(path + ': ' + names);
            symbolCount += info.symbols.length;
        }
    }

    return 'Project has ' + fileCount + ' indexed files, ' + symbolCount + ' symbols.\n' + parts.join('\n');
}

function findSymbol(name) {
    var results = [];
    for (var path in projectSymbols.files) {
        if (!projectSymbols.files.hasOwnProperty(path)) continue;
        var info = projectSymbols.files[path];
        for (var i = 0; i < info.symbols.length; i++) {
            if (info.symbols[i].name === name) {
                results.push({ path: path, symbol: info.symbols[i] });
            }
        }
    }
    return results;
}

function hoverInfo(path, line, col) {
    var file = projectSymbols.files[path];
    if (!file) return null;

    var content = '';
    for (var k in projectSymbols.files) {
        if (k === path) continue;
        if (projectSymbols.files.hasOwnProperty(k)) {
            var other = projectSymbols.files[k];
            for (var i = 0; i < other.symbols.length; i++) {
                if (other.symbols[i].line === line && other.symbols[i].col === col) {
                    content = '// defined in ' + k + ':' + other.symbols[i].line;
                }
            }
        }
    }

    for (var j = 0; j < file.symbols.length; j++) {
        var sym = file.symbols[j];
        if (sym.line === line) {
            var lineContent = sym.name;
            content = '`' + sym.name + '` (' + sym.kind + ') at ' + path + ':' + sym.line;
        }
    }

    return content || null;
}

async function indexOpenFiles() {
    var editor = document.getElementById('code-editor');
    var path = editor && editor.dataset.path;
    if (path) {
        var content = editor.textContent || '';
        indexFile(path, content);
    }
}

async function buildProjectIndex() {
    projectSymbols = { files: {}, langExtensions: {} };
    await indexOpenFiles();

    try {
        var entries = await invoke('get_project_structure', { workspace: state.workspacePath });
        if (entries && entries.length > 0) {
            var sourceFiles = entries.filter(function(e) {
                return !e.is_directory && e.size < 50000 && langFromPath(e.path);
            });

            for (var i = 0; i < sourceFiles.length; i++) {
                var e = sourceFiles[i];
                if (projectSymbols.files[e.path]) continue;
                try {
                    var content = await invoke('read_file', { path: e.path });
                    if (content) {
                        indexFile(e.path, content);
                    }
                } catch (err) {
                    // skip files that can't be read
                }
            }
        }
    } catch (e) {
        // workspace not set or error
    }
    return projectSymbols;
}

window.projectSymbols = projectSymbols;
window.indexFile = indexFile;
window.removeFileIndex = removeFileIndex;
window.buildProjectSummary = buildProjectSummary;
window.findSymbol = findSymbol;
window.hoverInfo = hoverInfo;
window.buildProjectIndex = buildProjectIndex;
window.indexOpenFiles = indexOpenFiles;
