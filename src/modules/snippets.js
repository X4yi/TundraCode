var SNIPPETS = {
    rust: [
        { prefix: 'fn', body: 'fn ${1:name}(${2:args}) -> ${3:type} {\n    ${0}\n}', label: 'fn name() -> type' },
        { prefix: 'impl', body: 'impl ${1:Type} {\n    ${0}\n}', label: 'impl Type' },
        { prefix: 'let', body: 'let ${1:var} = ${0};', label: 'let binding' },
        { prefix: 'match', body: 'match ${1:expr} {\n    ${2:pattern} => ${0},\n}', label: 'match expression' },
        { prefix: 'for', body: 'for ${1:item} in ${2:iter} {\n    ${0}\n}', label: 'for loop' },
        { prefix: 'if', body: 'if ${1:condition} {\n    ${0}\n}', label: 'if expression' },
        { prefix: 'struct', body: 'struct ${1:Name} {\n    ${0}\n}', label: 'struct definition' },
        { prefix: 'enum', body: 'enum ${1:Name} {\n    ${0}\n}', label: 'enum definition' },
        { prefix: 'trait', body: 'trait ${1:Name} {\n    ${0}\n}', label: 'trait definition' },
        { prefix: 'pub', body: 'pub ${1:fn} ${2:name}(${3:args}) {\n    ${0}\n}', label: 'pub fn' },
        { prefix: 'unsafe', body: 'unsafe {\n    ${0}\n}', label: 'unsafe block' },
        { prefix: 'mod', body: 'mod ${1:name} {\n    ${0}\n}', label: 'module definition' },
        { prefix: 'use', body: 'use ${1:crate}::${0};', label: 'use statement' },
        { prefix: 'main', body: 'fn main() {\n    ${0}\n}', label: 'fn main' },
        { prefix: 'async', body: 'async fn ${1:name}(${2:args}) -> ${3:type} {\n    ${0}\n}', label: 'async fn' },
        { prefix: 'dbg', body: 'dbg!(${0});', label: 'dbg! macro' },
    ],
    javascript: [
        { prefix: 'fn', body: 'function ${1:name}(${2:args}) {\n    ${0}\n}', label: 'function declaration' },
        { prefix: 'func', body: 'function ${1:name}(${2:args}) {\n    ${0}\n}', label: 'function declaration' },
        { prefix: 'const', body: 'const ${1:name} = ${0};', label: 'const declaration' },
        { prefix: 'let', body: 'let ${1:name} = ${0};', label: 'let declaration' },
        { prefix: 'var', body: 'var ${1:name} = ${0};', label: 'var declaration' },
        { prefix: 'for', body: 'for (let ${1:i} = 0; ${1:i} < ${2:n}; ${1:i}++) {\n    ${0}\n}', label: 'for loop' },
        { prefix: 'forof', body: 'for (const ${1:item} of ${2:iter}) {\n    ${0}\n}', label: 'for...of loop' },
        { prefix: 'forin', body: 'for (const ${1:key} in ${2:obj}) {\n    ${0}\n}', label: 'for...in loop' },
        { prefix: 'if', body: 'if (${1:condition}) {\n    ${0}\n}', label: 'if statement' },
        { prefix: 'else', body: '} else {\n    ${0}\n}', label: 'else statement' },
        { prefix: 'elseif', body: '} else if (${1:condition}) {\n    ${0}\n}', label: 'else if' },
        { prefix: 'while', body: 'while (${1:condition}) {\n    ${0}\n}', label: 'while loop' },
        { prefix: 'class', body: 'class ${1:Name} {\n    constructor(${2:args}) {\n        ${0}\n    }\n}', label: 'class definition' },
        { prefix: 'arrow', body: '(${1:args}) => {\n    ${0}\n}', label: 'arrow function' },
        { prefix: 'try', body: 'try {\n    ${0}\n} catch (${1:error}) {\n    ${2}\n}', label: 'try/catch' },
        { prefix: 'switch', body: 'switch (${1:expr}) {\n    case ${2:value}:\n        ${0}\n        break;\n    default:\n        break;\n}', label: 'switch statement' },
        { prefix: 'import', body: 'import { ${1:name} } from "${0}";', label: 'import statement' },
        { prefix: 'export', body: 'export ${1:function} ${2:name}(${3:args}) {\n    ${0}\n}', label: 'export declaration' },
        { prefix: 'log', body: 'console.log(${0});', label: 'console.log' },
        { prefix: 'async', body: 'async function ${1:name}(${2:args}) {\n    ${0}\n}', label: 'async function' },
        { prefix: 'await', body: 'await ${0};', label: 'await expression' },
        { prefix: 'promise', body: 'new Promise((resolve, reject) => {\n    ${0}\n});', label: 'new Promise' },
        { prefix: 'addEventListener', body: '${1:element}.addEventListener("${2:event}", (${3:e}) => {\n    ${0}\n});', label: 'addEventListener' },
    ],
    typescript: [],
    python: [
        { prefix: 'def', body: 'def ${1:name}(${2:args}):\n    ${0}', label: 'function definition' },
        { prefix: 'class', body: 'class ${1:Name}:\n    def __init__(self, ${2:args}):\n        ${0}', label: 'class definition' },
        { prefix: 'for', body: 'for ${1:item} in ${2:iterable}:\n    ${0}', label: 'for loop' },
        { prefix: 'if', body: 'if ${1:condition}:\n    ${0}', label: 'if statement' },
        { prefix: 'elif', body: 'elif ${1:condition}:\n    ${0}', label: 'elif statement' },
        { prefix: 'while', body: 'while ${1:condition}:\n    ${0}', label: 'while loop' },
        { prefix: 'with', body: 'with ${1:expr} as ${2:var}:\n    ${0}', label: 'with statement' },
        { prefix: 'try', body: 'try:\n    ${0}\nexcept ${1:Exception} as ${2:e}:\n    ${3}\nfinally:\n    ${4}', label: 'try/except/finally' },
        { prefix: 'import', body: 'import ${0}', label: 'import statement' },
        { prefix: 'from', body: 'from ${1:module} import ${0}', label: 'from import' },
        { prefix: 'lambda', body: 'lambda ${1:args}: ${0}', label: 'lambda expression' },
        { prefix: 'print', body: 'print(${0})', label: 'print function' },
        { prefix: 'main', body: 'def main():\n    ${0}\n\nif __name__ == "__main__":\n    main()', label: 'main function' },
        { prefix: 'async', body: 'async def ${1:name}(${2:args}):\n    ${0}', label: 'async function' },
        { prefix: 'await', body: 'await ${0}', label: 'await expression' },
        { prefix: 'listcomp', body: '[${1:expr} for ${2:item} in ${3:iterable}]', label: 'list comprehension' },
    ],
    html: [
        { prefix: 'div', body: '<div>${0}</div>', label: '<div> element' },
        { prefix: 'span', body: '<span>${0}</span>', label: '<span> element' },
        { prefix: 'a', body: '<a href="${1:#}">${0}</a>', label: '<a> link' },
        { prefix: 'img', body: '<img src="${1:url}" alt="${2:description}" />', label: '<img> tag' },
        { prefix: 'input', body: '<input type="${1:text}" name="${2:name}" id="${3:id}" />', label: '<input> element' },
        { prefix: 'form', body: '<form action="${1:/}" method="${2:post}">\n    ${0}\n</form>', label: '<form> element' },
        { prefix: 'button', body: '<button type="${1:button}" onclick="${2}">${0}</button>', label: '<button> element' },
        { prefix: 'ul', body: '<ul>\n    <li>${0}</li>\n</ul>', label: '<ul> list' },
        { prefix: 'li', body: '<li>${0}</li>', label: '<li> item' },
        { prefix: 'table', body: '<table>\n    <tr>\n        <th>${0}</th>\n    </tr>\n</table>', label: '<table> element' },
        { prefix: 'script', body: '<script>\n    ${0}\n</script>', label: '<script> tag' },
        { prefix: 'style', body: '<style>\n    ${0}\n</style>', label: '<style> tag' },
        { prefix: 'link', body: '<link rel="stylesheet" href="${0}" />', label: '<link> stylesheet' },
        { prefix: 'meta', body: '<meta charset="${1:UTF-8}" name="${2:viewport}" content="${0}" />', label: '<meta> tag' },
        { prefix: 'doc', body: '<!DOCTYPE html>\n<html lang="en">\n<head>\n    <meta charset="UTF-8">\n    <meta name="viewport" content="width=device-width, initial-scale=1">\n    <title>${1:Document}</title>\n</head>\n<body>\n    ${0}\n</body>\n</html>', label: 'HTML document' },
    ],
    css: [
        { prefix: 'flex', body: 'display: flex;\njustify-content: ${1:center};\nalign-items: ${2:center};', label: 'flexbox layout' },
        { prefix: 'grid', body: 'display: grid;\ngrid-template-columns: ${1:1fr};\ngap: ${2:1rem};', label: 'CSS grid' },
        { prefix: 'margin', body: 'margin: ${1:0};', label: 'margin shorthand' },
        { prefix: 'padding', body: 'padding: ${1:0};', label: 'padding shorthand' },
        { prefix: 'border', body: 'border: ${1:1px} solid ${2:#000};', label: 'border shorthand' },
        { prefix: 'bg', body: 'background: ${1:#fff};', label: 'background shorthand' },
        { prefix: 'bgc', body: 'background-color: ${1:#fff};', label: 'background color' },
        { prefix: 'color', body: 'color: ${1:#000};', label: 'text color' },
        { prefix: 'font', body: 'font-size: ${1:16px};\nfont-weight: ${2:400};', label: 'font properties' },
        { prefix: 'pos', body: 'position: ${1:relative};', label: 'position property' },
        { prefix: 'media', body: '@media (${1:max-width}: ${2:768px}) {\n    ${0}\n}', label: 'media query' },
        { prefix: 'hover', body: '&:hover {\n    ${0}\n}', label: 'hover pseudo-class' },
        { prefix: 'before', body: '&::before {\n    content: "${0}";\n}', label: 'before pseudo-element' },
        { prefix: 'after', body: '&::after {\n    content: "${0}";\n}', label: 'after pseudo-element' },
        { prefix: 'transition', body: 'transition: ${1:all} ${2:0.3s} ${3:ease};', label: 'CSS transition' },
        { prefix: 'animation', body: '@keyframes ${1:name} {\n    0% { ${2} }\n    100% { ${3} }\n}\n\nanimation: ${1:name} ${4:1s} ${5:ease};', label: 'CSS animation' },
        { prefix: 'transform', body: 'transform: ${1:translateX(${2:0})};', label: 'transform property' },
        { prefix: 'shadow', body: 'box-shadow: ${1:0 2px 4px rgba(0,0,0,0.1)};', label: 'box shadow' },
    ],
    cpp: [
        { prefix: 'for', body: 'for (int ${1:i} = 0; ${1:i} < ${2:n}; ${1:i}++) {\n    ${0}\n}', label: 'for loop' },
        { prefix: 'if', body: 'if (${1:condition}) {\n    ${0}\n}', label: 'if statement' },
        { prefix: 'class', body: 'class ${1:Name} {\npublic:\n    ${1:Name}() {}\n    ~${1:Name}() {}\n\nprivate:\n    ${0}\n};', label: 'class definition' },
        { prefix: 'struct', body: 'struct ${1:Name} {\n    ${1:Name}() = default;\n    ${0}\n};', label: 'struct definition' },
        { prefix: 'template', body: 'template<typename ${1:T}>\n${2:class} ${3:Name} {\n    ${0}\n};', label: 'template declaration' },
        { prefix: 'namespace', body: 'namespace ${1:name} {\n    ${0}\n} // namespace ${1:name}', label: 'namespace' },
        { prefix: 'auto', body: 'auto ${1:var} = ${0};', label: 'auto variable' },
        { prefix: 'constexpr', body: 'constexpr auto ${1:var} = ${0};', label: 'constexpr variable' },
        { prefix: 'include', body: '#include <${0}>', label: '#include directive' },
        { prefix: 'main', body: 'int main(int argc, char* argv[]) {\n    ${0}\n    return 0;\n}', label: 'main function' },
        { prefix: 'print', body: 'std::cout << ${0} << std::endl;', label: 'std::cout print' },
        { prefix: 'vec', body: 'std::vector<${1:int}> ${2:v} = {${0}};', label: 'std::vector' },
        { prefix: 'map', body: 'std::map<${1:string}, ${2:int}> ${3:m};', label: 'std::map' },
        { prefix: 'smartptr', body: 'std::unique_ptr<${1:int}> ${2:ptr} = std::make_unique<${1:int}}>(${0});', label: 'unique_ptr' },
        { prefix: 'sharedptr', body: 'std::shared_ptr<${1:int}> ${2:ptr} = std::make_shared<${1:int}}>(${0});', label: 'shared_ptr' },
        { prefix: 'fn', body: '${1:return_type} ${2:name}(${3:args}) {\n    ${0}\n}', label: 'function definition' },
    ],
    c: [
        { prefix: 'for', body: 'for (int ${1:i} = 0; ${1:i} < ${2:n}; ${1:i}++) {\n    ${0}\n}', label: 'for loop' },
        { prefix: 'if', body: 'if (${1:condition}) {\n    ${0}\n}', label: 'if statement' },
        { prefix: 'while', body: 'while (${1:condition}) {\n    ${0}\n}', label: 'while loop' },
        { prefix: 'struct', body: 'struct ${1:name} {\n    ${0}\n};', label: 'struct definition' },
        { prefix: 'enum', body: 'typedef enum {\n    ${1:VALUE},\n} ${2:Name};', label: 'enum definition' },
        { prefix: 'include', body: '#include <${0}>', label: '#include directive' },
        { prefix: 'define', body: '#define ${1:NAME} ${0}', label: '#define macro' },
        { prefix: 'typedef', body: 'typedef ${1:int} ${2:name};', label: 'typedef declaration' },
        { prefix: 'main', body: 'int main(int argc, char *argv[]) {\n    ${0}\n    return 0;\n}', label: 'main function' },
        { prefix: 'printf', body: 'printf("${0}");', label: 'printf function' },
        { prefix: 'malloc', body: '${1:type}* ${2:ptr} = malloc(sizeof(${1:type}));', label: 'malloc' },
        { prefix: 'free', body: 'free(${0});', label: 'free function' },
        { prefix: 'fn', body: '${1:return_type} ${2:name}(${3:args}) {\n    ${0}\n}', label: 'function definition' },
        { prefix: 'sizeof', body: 'sizeof(${0})', label: 'sizeof operator' },
        { prefix: 'null', body: 'NULL', label: 'NULL constant' },
    ],
};

var KEYWORDS = {
    rust: ['fn', 'let', 'mut', 'const', 'if', 'else', 'while', 'for', 'in', 'return', 'struct', 'enum', 'trait', 'impl', 'self', 'super', 'crate', 'pub', 'use', 'mod', 'type', 'where', 'match', 'as', 'ref', 'move', 'async', 'await', 'unsafe', 'dyn', 'true', 'false', 'None', 'Some', 'Ok', 'Err'],
    javascript: ['function', 'const', 'let', 'var', 'if', 'else', 'switch', 'case', 'break', 'continue', 'return', 'for', 'while', 'do', 'try', 'catch', 'finally', 'throw', 'new', 'this', 'typeof', 'instanceof', 'class', 'extends', 'import', 'export', 'default', 'from', 'async', 'await', 'yield', 'true', 'false', 'null', 'undefined', 'NaN', 'delete', 'in', 'of'],
    typescript: ['function', 'const', 'let', 'var', 'if', 'else', 'switch', 'case', 'break', 'continue', 'return', 'for', 'while', 'do', 'try', 'catch', 'finally', 'throw', 'new', 'this', 'typeof', 'instanceof', 'class', 'extends', 'implements', 'import', 'export', 'default', 'from', 'async', 'await', 'yield', 'true', 'false', 'null', 'undefined', 'NaN', 'interface', 'type', 'enum', 'namespace', 'as', 'in', 'of', 'keyof', 'readonly', 'abstract', 'private', 'protected', 'public', 'static'],
    python: ['def', 'class', 'if', 'elif', 'else', 'for', 'while', 'try', 'except', 'finally', 'with', 'as', 'import', 'from', 'return', 'yield', 'lambda', 'pass', 'break', 'continue', 'raise', 'assert', 'del', 'global', 'nonlocal', 'True', 'False', 'None', 'and', 'or', 'not', 'in', 'is', 'async', 'await'],
    html: ['html', 'head', 'body', 'div', 'span', 'p', 'a', 'img', 'input', 'form', 'button', 'ul', 'ol', 'li', 'table', 'tr', 'td', 'th', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'header', 'footer', 'section', 'article', 'nav', 'main', 'aside', 'meta', 'link', 'script', 'style'],
    css: ['display', 'position', 'margin', 'padding', 'border', 'background', 'color', 'font-size', 'font-weight', 'width', 'height', 'top', 'left', 'right', 'bottom', 'flex', 'grid', 'align-items', 'justify-content', 'gap', 'overflow', 'z-index', 'opacity', 'transform', 'transition', 'animation', 'box-shadow', 'text-align', 'float', 'clear', 'visibility', 'cursor'],
    cpp: ['int', 'float', 'double', 'char', 'bool', 'void', 'auto', 'const', 'constexpr', 'static', 'virtual', 'override', 'final', 'public', 'private', 'protected', 'class', 'struct', 'enum', 'union', 'namespace', 'template', 'typename', 'using', 'typedef', 'if', 'else', 'switch', 'case', 'break', 'continue', 'return', 'for', 'while', 'do', 'try', 'catch', 'throw', 'new', 'delete', 'this', 'true', 'false', 'nullptr', 'include', 'define', 'sizeof'],
    c: ['int', 'float', 'double', 'char', 'void', 'long', 'short', 'unsigned', 'signed', 'const', 'static', 'extern', 'volatile', 'register', 'struct', 'enum', 'union', 'typedef', 'if', 'else', 'switch', 'case', 'break', 'continue', 'return', 'for', 'while', 'do', 'goto', 'sizeof', 'NULL', 'include', 'define', 'main'],
};

function getLangForSnippets() {
    var langId = window.hlState && window.hlState.currentLangId;
    if (!langId) {
        try { langId = syntax.detectLanguage(state.currentFilePath || ''); } catch (e) {}
    }
    if (!langId) return null;
    if (langId in SNIPPETS) return langId;
    if (langId === 'tsx') return 'javascript';
    return null;
}

function getSnippetsForLang(lang) {
    return SNIPPETS[lang] || [];
}

function getKeywordsForLang(lang) {
    return KEYWORDS[lang] || [];
}

function getWordBeforeCursor(editor, cursorOffset) {
    var text = editor.textContent || '';
    var start = cursorOffset;
    while (start > 0 && /[a-zA-Z_0-9]/.test(text[start - 1])) start--;
    return { word: text.substring(start, cursorOffset), start: start, end: cursorOffset };
}

function getCompletions(editor, cursorOffset) {
    var lang = getLangForSnippets();
    if (!lang) return [];

    var before = getWordBeforeCursor(editor, cursorOffset);
    var word = before.word.toLowerCase();

    var results = [];
    var seen = {};

    var snippets = getSnippetsForLang(lang);
    for (var i = 0; i < snippets.length; i++) {
        var s = snippets[i];
        if (s.prefix.indexOf(word) === 0 || (word.length >= 2 && s.prefix.indexOf(word) > 0)) {
            if (!seen[s.prefix]) {
                results.push({ type: 'snippet', prefix: s.prefix, body: s.body, label: s.label || s.prefix, kind: 'snip' });
                seen[s.prefix] = true;
            }
        }
    }

    var keywords = getKeywordsForLang(lang);
    for (var k = 0; k < keywords.length; k++) {
        var kw = keywords[k];
        if (kw.indexOf(word) === 0 && kw !== word) {
            if (!seen[kw]) {
                results.push({ type: 'keyword', prefix: kw, body: kw, label: kw, kind: 'kw' });
                seen[kw] = true;
            }
        }
    }

    results.sort(function(a, b) {
        var aExact = a.prefix === word ? 0 : 1;
        var bExact = b.prefix === word ? 0 : 1;
        if (aExact !== bExact) return aExact - bExact;
        return a.prefix.localeCompare(b.prefix);
    });

    return results.slice(0, 20);
}

function expandSnippet(body, editor, offset) {
    var tabStopRegex = /\$\{(\d+):([^}]*)\}|\$(\d+)|\${(\d+)}/g;
    var parts = [];
    var lastIndex = 0;
    var match;
    var placeholders = {};

    while ((match = tabStopRegex.exec(body)) !== null) {
        if (match.index > lastIndex) {
            parts.push({ text: body.substring(lastIndex, match.index), tabStop: -1 });
        }
        var index = parseInt(match[1] || match[3] || match[4] || '0');
        var defaultValue = match[2] || '';
        parts.push({ text: defaultValue, tabStop: index });
        if (!placeholders[index]) placeholders[index] = { index: index, start: 0, end: 0 };
        lastIndex = match.index + match[0].length;
    }
    if (lastIndex < body.length) {
        parts.push({ text: body.substring(lastIndex), tabStop: -1 });
    }

    var content = editor.textContent || '';
    var before = content.substring(0, offset);
    var after = content.substring(offset);
    var text = '';

    for (var p = 0; p < parts.length; p++) {
        text += parts[p].text;
    }

    editor.textContent = before + text + after;

    var finalOffset = before.length;
    for (var p2 = 0; p2 < parts.length; p2++) {
        if (parts[p2].tabStop >= 0) {
            var sel = document.createRange ? document.createRange() : null;
            var selObj = window.getSelection();
            if (sel && selObj) {
                var node = editor.firstChild || editor;
                try {
                    sel.setStart(node, finalOffset);
                    sel.setEnd(node, finalOffset + parts[p2].text.length);
                    selObj.removeAllRanges();
                    selObj.addRange(sel);
                } catch (e) {
                    setEditorCursor(finalOffset + parts[p2].text.length);
                }
            }
            break;
        }
        finalOffset += parts[p2].text.length;
    }
}

window.SNIPPETS = SNIPPETS;
window.getCompletions = getCompletions;
window.expandSnippet = expandSnippet;
window.getWordBeforeCursor = getWordBeforeCursor;
