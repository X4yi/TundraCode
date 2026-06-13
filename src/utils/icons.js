

var FILE_ICONS = {};
var ICONS_MANIFEST = null;
var ICONS_DIR = null;

function setIconsManifest(manifest, iconsDir) {
    ICONS_MANIFEST = manifest;
    ICONS_DIR = iconsDir;
}

function loadIcon(name) {
    if (ICONS_MANIFEST && ICONS_DIR) {
        var iconFile = ICONS_MANIFEST.icons[name];
        if (iconFile) {
            return ICONS_DIR + '/' + iconFile;
        }
    }

    var paths = {
        '_default_file': 'icons/default_file.svg',
        '_default_folder': 'icons/default_folder.svg',
        '_default_folder_opened': 'icons/default_folder_opened.svg',
        'rs': 'icons/file_type_rust.svg',
        'js': 'icons/file_type_javascript.svg',
        'jsx': 'icons/file_type_javascript.svg',
        'ts': 'icons/file_type_typescript.svg',
        'tsx': 'icons/file_type_typescript.svg',
        'py': 'icons/file_type_python.svg',
        'java': 'icons/file_type_java.svg',
        'html': 'icons/file_type_html.svg',
        'css': 'icons/file_type_css.svg',
        'json': 'icons/file_type_json.svg',
        'md': 'icons/file_type_markdown.svg',
        'toml': 'icons/file_type_toml.svg',
        'yaml': 'icons/file_type_yaml.svg',
        'yml': 'icons/file_type_yaml.svg',
        'c': 'icons/file_type_c.svg',
        'cpp': 'icons/file_type_cpp.svg',
        'go': 'icons/file_type_go.svg',
        'php': 'icons/file_type_php.svg',
        'rb': 'icons/file_type_ruby.svg',
        'swift': 'icons/file_type_swift.svg',
        'kt': 'icons/file_type_kotlin.svg',
        'lua': 'icons/file_type_lua.svg',
        'sh': 'icons/file_type_shell.svg',
        'bash': 'icons/file_type_shell.svg',
        'git': 'icons/file_type_git.svg',
        'docker': 'icons/file_type_docker.svg',
    };
    return paths[name] || paths['_default_file'];
}

function getFileIcon(filename, isDirectory) {
    if (isDirectory) {
        return loadIcon('_default_folder');
    }

    if (ICONS_MANIFEST && ICONS_MANIFEST.filenames) {
        var iconFile = ICONS_MANIFEST.filenames[filename];
        if (iconFile) {
            return ICONS_DIR + '/' + iconFile;
        }
    }

    if (filename === '.gitignore' || filename === '.gitattributes' || filename === '.gitmodules') {
        return loadIcon('git');
    }
    if (filename === 'Dockerfile' || filename.startsWith('Dockerfile.')) {
        return loadIcon('docker');
    }
    if (filename === 'Cargo.toml' || filename === 'Cargo.lock') {
        return loadIcon('rs');
    }
    if (filename === 'package.json' || filename === 'package-lock.json') {
        return loadIcon('js');
    }
    if (filename === 'tsconfig.json') {
        return loadIcon('ts');
    }
    if (filename === 'go.mod' || filename === 'go.sum') {
        return loadIcon('go');
    }

    var parts = filename.split('.');
    if (parts.length > 1) {
        var ext = parts[parts.length - 1].toLowerCase();
        return loadIcon(ext);
    }

    return loadIcon('_default_file');
}

function wrapIcon(svgPath) {
    return '<img src="' + svgPath + '" width="16" height="16" class="file-icon" style="filter:saturate(1.3)" alt="">';
}
