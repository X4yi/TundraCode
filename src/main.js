
var invoke;
var tauriEvent;
if (window.__TAURI__ && window.__TAURI__.core) {
    invoke = window.__TAURI__.core.invoke;
    tauriEvent = window.__TAURI__.event;
} else {
    console.warn('Tauri API no disponible');
    invoke = async function() { throw new Error('Tauri no disponible'); };
    tauriEvent = { listen: async function() {} };
}


var state = {
    workspacePath: null,
    openFiles: [],
    activeFile: null,
    agentsPanelVisible: true,
    activeAgentTab: 'plan',
    agentMode: 'plan',
    modifiedFiles: new Set(),
    fileContents: new Map(),
    providers: [],
    connectedProviders: {},
    activeSettingsTab: 'providers',
    settingsModelProvider: 'openai',
    selectedProviderId: null,
    selectedModelId: null,
    reasoningEffort: 'low',
    conversationHistory: [],
    isLoadingCompletion: false,
    providerModels: {},
    providerModelCache: {},
    tokenUsage: { session: 0, total: 0 },
    markdownPreviewActive: false,
    explorer: {
        expandedFolders: new Set(),
        scrollPosition: 0,
    },
    toolLogVisible: false,
    toolCalls: [],
    tokenBreakdown: { input: 0, output: 0, total: 0 },
    lastRequestTokens: 0,
    pendingDiffs: {},
    taskProgress: {
        currentTask: null,
        totalTasks: 0,
        tasks: [],
        paused: false,
        pausedProposals: [],
        runId: null,
    },
    modelContextInfo: null,
    hasStartedToolCall: false,
    sessionTitle: null,
    currentSessionFile: null,
    undo: {
        stacks: new Map(),
        lastContent: '',
        lastPushTime: 0,
    },
    historySnapshots: [],
};

var hlState = {
    requestId: 0,
    scheduled: false,
    debounceTimer: null,
    sizeCap: 500 * 1024,
    currentLangId: null,
    currentFilePath: null,
};

function getEditorContent() {
    var editor = document.getElementById('code-editor');
    if (editor.tagName === 'TEXTAREA') return editor.value;
    return editor.textContent || '';
}

function setEditorContent(content) {
    var editor = document.getElementById('code-editor');
    if (!editor) return;
    if (editor.tagName === 'TEXTAREA') {
        editor.value = content;
    } else {
        editor.textContent = content;
    }
}

function getEditorSelection() {
    var editor = document.getElementById('code-editor');
    if (editor.tagName === 'TEXTAREA') {
        return { start: editor.selectionStart, end: editor.selectionEnd };
    }
    var sel = window.getSelection();
    if (sel.rangeCount > 0) {
        var range = sel.getRangeAt(0);
        var preRange = range.cloneRange();
        preRange.selectNodeContents(editor);
        preRange.setEnd(range.startContainer, range.startOffset);
        return { start: preRange.toString().length, end: preRange.toString().length + range.toString().length };
    }
    return { start: 0, end: 0 };
}

function setEditorSelection(start, end) {
    var editor = document.getElementById('code-editor');
    if (editor.tagName === 'TEXTAREA') {
        editor.selectionStart = start;
        editor.selectionEnd = end;
        return;
    }
    var sel = window.getSelection();
    var range = document.createRange();
    var walker = document.createTreeWalker(editor, NodeFilter.SHOW_TEXT, null, false);
    var charCount = 0;
    var startNode = null, startOffset = 0, endNode = null, endOffset = 0;
    while (walker.nextNode()) {
        var node = walker.currentNode;
        var nodeLen = node.textContent.length;
        if (!startNode && charCount + nodeLen >= start) {
            startNode = node;
            startOffset = start - charCount;
        }
        if (!endNode && charCount + nodeLen >= end) {
            endNode = node;
            endOffset = end - charCount;
            break;
        }
        charCount += nodeLen;
    }
    if (startNode && endNode) {
        range.setStart(startNode, startOffset);
        range.setEnd(endNode, endOffset);
        sel.removeAllRanges();
        sel.addRange(range);
    }
}

function clearHighlights() {
    var codeEl = document.getElementById('highlights-code');
    if (codeEl) codeEl.innerHTML = '';
    var pre = document.querySelector('.highlights-pre');
    if (pre) pre.style.transform = 'translate(0, 0)';
    var editor = document.getElementById('code-editor');
    if (editor) editor.classList.remove('highlighted');
}

function isMarkdownFile(path) {
    if (!path) return false;
    var ext = path.toLowerCase().split('.').pop();
    return ext === 'md' || ext === 'markdown' || ext === 'mdx';
}

function updateMarkdownPreviewToggle() {
    var path = state.activeFile;
    var toggle = document.getElementById('md-preview-toggle');
    if (!toggle) return;

    if (isMarkdownFile(path)) {
        toggle.classList.remove('hidden');
    } else {
        toggle.classList.add('hidden');
        // If preview was active, turn it off
        if (state.markdownPreviewActive) {
            toggleMarkdownPreview();
        }
    }
}

function toggleMarkdownPreview() {
    var checkbox = document.getElementById('md-preview-checkbox');
    if (!checkbox) return;

    state.markdownPreviewActive = checkbox.checked;

    var editor = document.getElementById('code-editor');
    var lineNumbers = document.getElementById('line-numbers');
    var highlights = document.querySelector('.editor-highlights');
    var previewContainer = document.getElementById('md-preview-container');

    if (state.markdownPreviewActive) {
        // Show preview, hide editor
        editor.classList.add('hidden');
        lineNumbers.classList.add('hidden');
        highlights.classList.add('hidden');
        previewContainer.classList.remove('hidden');
        refreshMarkdownPreview();
    } else {
        // Show editor, hide preview
        editor.classList.remove('hidden');
        lineNumbers.classList.remove('hidden');
        highlights.classList.remove('hidden');
        previewContainer.classList.add('hidden');
        // Restore syntax highlighting
        scheduleHighlight();
    }
}

function refreshMarkdownPreview() {
    if (!state.markdownPreviewActive) return;

    var path = state.activeFile;
    if (!path) return;

    var content = state.fileContents.get(path) || '';
    var previewContainer = document.getElementById('md-preview-container');
    if (previewContainer) {
        previewContainer.innerHTML = renderMarkdown(content);
    }
}

function getExplorerStorageKey() {
    return 'tundracode_explorer_' + (state.workspacePath || '').replace(/[^a-zA-Z0-9]/g, '_');
}

function saveExplorerState() {
    if (!state.workspacePath) return;
    var key = getExplorerStorageKey();
    var data = {
        expandedFolders: Array.from(state.explorer.expandedFolders),
        scrollPosition: state.explorer.scrollPosition,
    };
    try {
        localStorage.setItem(key, JSON.stringify(data));
    } catch (e) {
        console.warn('Failed to save explorer state:', e);
    }
}

function loadExplorerState() {
    if (!state.workspacePath) return;
    var key = getExplorerStorageKey();
    try {
        var data = JSON.parse(localStorage.getItem(key) || '{}');
        state.explorer.expandedFolders = new Set(data.expandedFolders || []);
        state.explorer.scrollPosition = data.scrollPosition || 0;
    } catch (e) {
        console.warn('Failed to load explorer state:', e);
        state.explorer.expandedFolders = new Set();
        state.explorer.scrollPosition = 0;
    }
}

function getTabStorageKey() {
    return 'tundracode_tabs_' + (state.workspacePath || '').replace(/[^a-zA-Z0-9]/g, '_');
}

function saveTabsState() {
    if (!state.workspacePath) return;
    var key = getTabStorageKey();
    var data = {
        openFiles: state.openFiles.map(function(f) { return { path: f.path, name: f.name, language: f.language }; }),
        activePath: state.activeFile,
    };
    try {
        localStorage.setItem(key, JSON.stringify(data));
    } catch (e) {
        console.warn('Failed to save tabs state:', e);
    }
}

function loadTabsState() {
    if (!state.workspacePath) return;
    var key = getTabStorageKey();
    try {
        var data = JSON.parse(localStorage.getItem(key) || '{}');
        return data;
    } catch (e) {
        console.warn('Failed to load tabs state:', e);
        return null;
    }
}

async function restoreTabs() {
    var data = loadTabsState();
    if (!data || !data.openFiles || data.openFiles.length === 0) return;

    for (var i = 0; i < data.openFiles.length; i++) {
        var file = data.openFiles[i];
        try {
            var result = await invoke('read_file', { path: file.path });
            state.openFiles.push({ path: file.path, name: file.name, language: file.language });
            state.fileContents.set(file.path, result.content);
        } catch (e) {
            console.warn('Failed to restore tab:', file.path, e);
        }
    }

    if (data.activePath && state.openFiles.some(function(f) { return f.path === data.activePath; })) {
        state.activeFile = data.activePath;
    } else if (state.openFiles.length > 0) {
        state.activeFile = state.openFiles[0].path;
    }

    if (state.activeFile) {
        showEditor();
        var editor = document.getElementById('code-editor');
        setEditorContent(state.fileContents.get(state.activeFile) || '');
        editor.dataset.path = state.activeFile;
        updateLineNumbers();
        scheduleHighlight();
    }

    renderTabs();
}

async function restoreExpandedFolders() {
    var treeContainer = document.getElementById('file-tree');
    if (!treeContainer) return;

    var paths = Array.from(state.explorer.expandedFolders);
    paths.sort(function(a, b) { return a.split('/').length - b.split('/').length; });

    for (var i = 0; i < paths.length; i++) {
        var folderPath = paths[i];
        var folderItem = treeContainer.querySelector('.tree-item.folder[data-path="' + folderPath + '"]');
        if (!folderItem) continue;

        var children = folderItem.nextElementSibling;
        if (children && children.classList.contains('tree-children')) {
            children.classList.remove('hidden');
            var chevron = folderItem.querySelector('.folder-chevron');
            if (chevron) chevron.style.transform = 'rotate(90deg)';
        } else {
            await loadDirectoryContents(folderPath, folderItem);
        }
    }
}

function scheduleHighlight() {
    if (hlState.debounceTimer) {
        clearTimeout(hlState.debounceTimer);
        hlState.debounceTimer = null;
    }
    hlState.debounceTimer = setTimeout(runHighlight, 32);
}

async function runHighlight() {
    hlState.scheduled = false;
    hlState.debounceTimer = null;
    if (!state.activeFile) {
        clearHighlights();
        return;
    }
    var path = state.activeFile;
    var langId = window.syntax ? window.syntax.detectLanguage(path) : null;
    var isNewFile = (hlState.currentLangId !== langId || hlState.currentFilePath !== path);
    hlState.currentLangId = langId;
    hlState.currentFilePath = path;
    if (!langId || !window.syntax || !window.syntax.isSupported(langId)) {
        clearHighlights();
        return;
    }
    var text = getEditorContent();
    if (text.length > hlState.sizeCap) {
        clearHighlights();
        return;
    }
    var myId = ++hlState.requestId;
    var result;
    try {
        result = await window.syntax.highlight(langId, text, isNewFile);
    } catch (e) {
        console.warn('highlight failed:', e);
        clearHighlights();
        return;
    }
    if (myId !== hlState.requestId) return;
    if (state.activeFile !== path) return;
    var codeEl = document.getElementById('highlights-code');
    if (codeEl) {
        codeEl.innerHTML = result.html;
        document.getElementById('code-editor').classList.add('highlighted');
    }
}


async function init() {
    try {
        var workspace = await invoke('get_workspace');
        if (workspace) {
            state.workspacePath = workspace;
            updateGitStatus();
        }
    } catch (e) {
        console.log('No hay workspace previo');
    }

    try {
        var windowInfo = await invoke('get_window_info');
        if (!windowInfo.decorations) {
            document.body.classList.add('no-decorations');
        }
    } catch (e) {}

    try {
        var lspServers = await invoke('detect_lsp_servers');
        updateLspStatus(lspServers);
    } catch (e) {}

    try {
        state.providers = await invoke('get_providers');
    } catch (e) {
        console.warn('Failed to load providers:', e);
    }

    setupEventListeners();
    setupAgentStreamListeners();
    SESSION_MANAGEMENT.setup();
    updateUI();
    updateExploreButton();
    loadModelSelector();
    loadPersistedModelCache();
    adjustAgentsPanelWidth();
    updateTokenDisplay();
    fetchModelContextInfo();
    buildProjectIndex();

    runStartupTasks();
    if (state.workspacePath) {
        restoreTabs();
    }
}

async function runStartupTasks() {
    try {
        var result = await invoke('run_startup_tasks');

        if (result.icons_dir) {
            try {
                var manifestPath = result.icons_dir + '/manifest.json';
                var manifestResponse = await fetch('file://' + manifestPath);
                var manifest = await manifestResponse.json();
                setIconsManifest(manifest, result.icons_dir);
            } catch (e) {
                console.warn('Failed to load icons manifest:', e);
            }
        }

        for (var task of result.tasks) {
            if (task.name.startsWith('models_') && task.status === 'ok') {
                var providerId = task.name.replace('models_', '');
                try {
                    var cache = await invoke('load_cached_models');
                    if (cache && cache[providerId]) {
                        state.providerModelCache[providerId] = cache[providerId];
                        state.providerModels[providerId] = cache[providerId].models.map(function(m) {
                            return m.id || m.name;
                        });
                    }
                } catch (e) {}
            }
        }
    } catch (e) {
        console.warn('Startup tasks failed:', e);
    }
}

function updateLspStatus(servers) {
    var lspStatusEl = document.getElementById('lsp-status');
    var activeServers = servers.filter(function(s) { return s.available; });
    
    if (activeServers.length > 0) {
        var names = activeServers.map(function(s) { return s.name; }).join(', ');
        lspStatusEl.innerHTML = '<span class="status-dot online"></span> LSP: ' + names;
    } else {
        lspStatusEl.innerHTML = '<span class="status-dot offline"></span> LSP: off';
    }
}

function setupEventListeners() {
    document.getElementById('explore-btn').addEventListener('click', toggleExplore);
    document.getElementById('change-workspace-btn').addEventListener('click', function(e) {
        e.stopPropagation();
        document.getElementById('explore-dropdown').classList.add('hidden');
        showWorkspacePicker();
    });
    document.getElementById('agents-toggle').addEventListener('click', toggleAgentsPanel);
    document.getElementById('settings-btn').addEventListener('click', toggleSettings);

    var editor = document.getElementById('code-editor');
    editor.addEventListener('input', onEditorInput);
    editor.addEventListener('keydown', onEditorKeydown);
    editor.addEventListener('click', updateCursorPosition);
    editor.addEventListener('keyup', updateCursorPosition);
    editor.addEventListener('keyup', onEditorKeyup);
    editor.addEventListener('scroll', syncLineNumbersScroll);
    editor.addEventListener('mouseover', onEditorHover);

    // Markdown preview toggle
    var mdPreviewCheckbox = document.getElementById('md-preview-checkbox');
    if (mdPreviewCheckbox) {
        mdPreviewCheckbox.addEventListener('change', toggleMarkdownPreview);
    }

    document.getElementById('agent-send').addEventListener('click', sendAskMessage);
    document.getElementById('reasoning-effort').addEventListener('change', function(e) {
        state.reasoningEffort = e.target.value || 'medium';
    });
    document.getElementById('agent-input').addEventListener('keydown', function(e) {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            sendAskMessage();
        }
    });

    var toolLogSearch = document.getElementById('tool-log-search');
    if (toolLogSearch) {
        toolLogSearch.addEventListener('input', filterToolLog);
        toolLogSearch.addEventListener('click', function(e) { e.stopPropagation(); });
    }

    var paletteInput = document.getElementById('command-palette-input');
    if (paletteInput) {
        paletteInput.addEventListener('input', function(e) {
            filterCommands(e.target.value);
            renderCommandPaletteList();
        });
        paletteInput.addEventListener('keydown', function(e) {
            if (e.key === 'ArrowDown' || e.key === 'ArrowUp' || e.key === 'Enter') {
                e.preventDefault();
            }
        });
    }

    document.addEventListener('click', function(e) {
        var dropdown = document.getElementById('explore-dropdown');
        var btn = document.getElementById('explore-btn');
        if (!dropdown.contains(e.target) && !btn.contains(e.target)) {
            dropdown.classList.add('hidden');
        }
    });

    document.addEventListener('keydown', function(e) {
        // Command palette: Ctrl+Shift+P or Cmd+Shift+P
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'p') {
            e.preventDefault();
            openCommandPalette();
            return;
        }
        
        // Close command palette on Escape
        if (e.key === 'Escape') {
            var palette = document.getElementById('command-palette');
            if (palette && !palette.classList.contains('hidden')) {
                e.preventDefault();
                closeCommandPalette();
                return;
            }
        }
        
        // Command palette navigation
        var palette = document.getElementById('command-palette');
        if (palette && !palette.classList.contains('hidden')) {
            if (e.key === 'ArrowDown') {
                e.preventDefault();
                commandPaletteSelected = (commandPaletteSelected + 1) % commandPaletteFiltered.length;
                renderCommandPaletteList();
                return;
            }
            if (e.key === 'ArrowUp') {
                e.preventDefault();
                commandPaletteSelected = (commandPaletteSelected - 1 + commandPaletteFiltered.length) % commandPaletteFiltered.length;
                renderCommandPaletteList();
                return;
            }
            if (e.key === 'Enter') {
                e.preventDefault();
                executeCommand(commandPaletteSelected);
                return;
            }
        }
        
        if ((e.ctrlKey || e.metaKey) && e.key === 'b') {
            e.preventDefault();
            toggleAgentsPanel();
        }
        if ((e.ctrlKey || e.metaKey) && e.key === 'd') {
            e.preventDefault();
            toggleDiffPopup();
        }
        if ((e.ctrlKey || e.metaKey) && e.key === ',') {
            e.preventDefault();
            toggleSettings();
        }
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'n') {
            e.preventDefault();
            newSession();
        }
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'l') {
            e.preventDefault();
            toggleToolLog();
        }
        if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'i') {
            e.preventDefault();
            document.getElementById('agent-input').focus();
        }
        if ((e.ctrlKey || e.metaKey) && e.key === 's') {
            e.preventDefault();
            saveCurrentFile();
        }
        if ((e.ctrlKey || e.metaKey) && e.key === 'w') {
            e.preventDefault();
            if (state.activeFile) closeFile(state.activeFile);
        }
    });

    setupResizeHandle();
    setupToolLogResizeHandle();
    setupModeSelector();
    setupAutoResize();
    setupModelSelector();

    window.addEventListener('resize', function() {
        syncLineNumbersScroll();
        scheduleHighlight();
    });
}


function setupResizeHandle() {
    var handle = document.getElementById('resize-handle');
    var panel = document.getElementById('agents-panel');
    var container = document.getElementById('main-container');
    var isResizing = false;

    handle.addEventListener('mousedown', function(e) {
        isResizing = true;
        handle.classList.add('active');
        document.body.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
        e.preventDefault();
    });

    document.addEventListener('mousemove', function(e) {
        if (!isResizing) return;
        var containerRect = container.getBoundingClientRect();
        var newWidth = containerRect.right - e.clientX;
        var containerWidth = containerRect.width;
        var percent = (newWidth / containerWidth) * 100;
        percent = Math.max(20, Math.min(70, percent));
        panel.style.width = percent + '%';
    });

    document.addEventListener('mouseup', function() {
        if (isResizing) {
            isResizing = false;
            handle.classList.remove('active');
            document.body.style.cursor = '';
            document.body.style.userSelect = '';
        }
    });
}

function setupToolLogResizeHandle() {
    var handle = document.getElementById('tool-log-resize-handle');
    var subpanel = document.getElementById('tool-log-subpanel');
    var isResizing = false;

    if (!handle || !subpanel) return;

    handle.addEventListener('mousedown', function(e) {
        isResizing = true;
        handle.classList.add('active');
        document.body.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
        e.preventDefault();
    });

    document.addEventListener('mousemove', function(e) {
        if (!isResizing) return;
        var subpanelRect = subpanel.getBoundingClientRect();
        var newWidth = subpanelRect.right - e.clientX;
        newWidth = Math.max(200, Math.min(500, newWidth));
        subpanel.style.width = newWidth + 'px';
    });

    document.addEventListener('mouseup', function() {
        if (isResizing) {
            isResizing = false;
            handle.classList.remove('active');
            document.body.style.cursor = '';
            document.body.style.userSelect = '';
        }
    });
}


async function openWorkspace(path) {
    try {
        if (state.workspacePath && state.workspacePath !== path) {
            resetWorkspaceState();
        }
        await invoke('open_workspace', { path: path });
        state.workspacePath = path;
        document.getElementById('explore-dropdown').classList.remove('hidden');
        loadFileTree();
        updateGitStatus();
        updateExploreButton();
    } catch (err) {
        console.error('Error abriendo workspace:', err);
    }
}


function toggleExplore(e) {
    e.stopPropagation();
    var dropdown = document.getElementById('explore-dropdown');
    var tree = document.getElementById('file-tree');

    if (dropdown.classList.contains('hidden')) {
        // Opening - state will be loaded in loadFileTree
        dropdown.classList.toggle('hidden');
        if (state.workspacePath) {
            loadFileTree();
        } else {
            showWorkspacePicker();
        }
    } else {
        // Closing - save scroll position
        if (tree) {
            state.explorer.scrollPosition = tree.scrollTop;
        }
        saveExplorerState();
        dropdown.classList.toggle('hidden');
    }
}

async function showWorkspacePicker() {
    try {
        var selected = await invoke('pick_directory');
        if (selected) {
            await openWorkspace(selected);
        }
    } catch (err) {
        console.error('Error seleccionando directorio:', err);
    }
}

async function loadFileTree(subPath) {
    subPath = subPath || '';
    var tree = document.getElementById('file-tree');
    tree.innerHTML = '<div class="tree-item">Cargando...</div>';

    try {
        var entries = await invoke('list_directory', { path: subPath });
        tree.innerHTML = '';

        if (subPath === '') {
            var header = document.createElement('div');
            header.className = 'dropdown-header';
            header.textContent = state.workspacePath ? state.workspacePath.split('/').pop() : 'Workspace';
            tree.appendChild(header);

            // Load and restore explorer state
            loadExplorerState();
        }

        entries.forEach(function(entry) { renderTreeItem(tree, entry, subPath); });

        // Restore expanded folders after rendering
        if (subPath === '') {
            await restoreExpandedFolders();
            // Restore scroll position
            if (state.explorer.scrollPosition > 0) {
                tree.scrollTop = state.explorer.scrollPosition;
            }
        }
    } catch (err) {
        tree.innerHTML = '<div class="tree-item">Error: ' + err + '</div>';
    }
}

function renderTreeItem(container, entry, parentPath) {
    var div = document.createElement('div');
    div.className = 'tree-item ' + (entry.is_directory ? 'folder' : '');
    div.style.paddingLeft = (12 + (parentPath.split('/').length - 1) * 16) + 'px';

    if (entry.is_directory) {
        div.setAttribute('data-path', entry.path);
    }

    var iconPath = getFileIcon(entry.name, entry.is_directory);
    var iconHtml = wrapIcon(iconPath);

    var chevronHtml = entry.is_directory
        ? '<span class="folder-chevron" style="display:inline-block;width:14px;text-align:center;transition:transform 0.15s;">▶</span>'
        : '<span style="width:14px;display:inline-block;"></span>';

    div.innerHTML = chevronHtml + iconHtml + '<span>' + entry.name + '</span>';

    if (entry.is_directory) {
        div.addEventListener('click', function(e) {
            e.stopPropagation();
            var childrenContainer = div.nextElementSibling;
            if (childrenContainer && childrenContainer.classList.contains('tree-children')) {
                var isHidden = childrenContainer.classList.toggle('hidden');
                var chevron = div.querySelector('.folder-chevron');
                if (chevron) chevron.style.transform = isHidden ? 'rotate(0deg)' : 'rotate(90deg)';

                if (!isHidden) {
                    state.explorer.expandedFolders.add(entry.path);
                } else {
                    state.explorer.expandedFolders.delete(entry.path);
                }
                saveExplorerState();
            } else {
                loadDirectoryContents(entry.path, div);
            }
        });
    } else {
        div.addEventListener('click', function() { openFile(entry.path, entry.name); });
    }

    container.appendChild(div);
}

async function loadDirectoryContents(path, parentElement) {
    try {
        var entries = await invoke('list_directory', { path: path });
        var childrenContainer = document.createElement('div');
        childrenContainer.className = 'tree-children';
        entries.forEach(function(entry) { renderTreeItem(childrenContainer, entry, path); });
        parentElement.after(childrenContainer);

        // Restore expanded state for nested folders
        if (state.explorer.expandedFolders.has(path)) {
            childrenContainer.classList.remove('hidden');
            var chevron = parentElement.querySelector('.folder-chevron');
            if (chevron) chevron.style.transform = 'rotate(90deg)';
        }
    } catch (err) {
        console.error('Error cargando directorio:', err);
    }
}


async function openFile(path, name) {
    if (state.openFiles.some(function(f) { return f.path === path; })) {
        setActiveFile(path);
        return;
    }

    try {
        var result = await invoke('read_file', { path: path });
        var fileObj = { path: path, name: name, language: result.language };
        state.openFiles.push(fileObj);
        state.activeFile = path;
        state.fileContents.set(path, result.content);

        renderTabs();
        showEditor();
        saveTabsState();

        var editor = document.getElementById('code-editor');
        setEditorContent(result.content);
        editor.dataset.path = path;
        updateLineNumbers();
        scheduleHighlight();
        updateMarkdownPreviewToggle();

        // Don't auto-close explorer - allow opening multiple files
        // document.getElementById('explore-dropdown').classList.add('hidden');
    } catch (err) {
        console.error('Error abriendo archivo:', err);
    }
}

function closeFile(path, event) {
    if (event) event.stopPropagation();

    var index = state.openFiles.findIndex(function(f) { return f.path === path; });
    if (index === -1) return;

    state.openFiles.splice(index, 1);
    state.modifiedFiles.delete(path);
    state.fileContents.delete(path);

    if (state.activeFile === path) {
        state.activeFile = state.openFiles.length > 0
            ? state.openFiles[Math.min(index, state.openFiles.length - 1)].path
            : null;

        if (state.activeFile) {
            var editor = document.getElementById('code-editor');
            setEditorContent(state.fileContents.get(state.activeFile) || '');
            editor.dataset.path = state.activeFile;
            updateLineNumbers();
            scheduleHighlight();
        } else {
            showPlaceholder();
            clearHighlights();
        }
    }

    renderTabs();
    saveTabsState();
}

function setActiveFile(path) {
    if (state.activeFile && state.activeFile !== path) {
        pushUndo(state.activeFile);
    }
    state.activeFile = path;
    state.undo.lastContent = state.fileContents.get(path) || '';
    var editor = document.getElementById('code-editor');
    setEditorContent(state.fileContents.get(path) || '');
    editor.dataset.path = path;
    renderTabs();
    updateLineNumbers();
    scheduleHighlight();
    updateMarkdownPreviewToggle();
    saveTabsState();
}

function renderTabs() {
    var container = document.getElementById('file-tabs');
    container.innerHTML = '';

    state.openFiles.forEach(function(file) {
        var tab = document.createElement('button');
        var isActive = file.path === state.activeFile;
        var isModified = state.modifiedFiles.has(file.path);

        tab.className = 'file-tab ' + (isActive ? 'active' : '') + (isModified ? ' modified' : '');
        tab.innerHTML = '<span>' + file.name + '</span><span class="tab-close">x</span>';
        tab.addEventListener('click', function() { setActiveFile(file.path); });
        tab.querySelector('.tab-close').addEventListener('click', function(e) { closeFile(file.path, e); });
        container.appendChild(tab);
    });
}


function getUndoStack(path) {
    if (!state.undo.stacks.has(path)) {
        state.undo.stacks.set(path, { undo: [], redo: [] });
    }
    return state.undo.stacks.get(path);
}

function pushUndo(path) {
    if (!path) return;
    var now = Date.now();
    if (now - state.undo.lastPushTime < 500) return;
    var content = state.fileContents.get(path) || '';
    if (content === state.undo.lastContent) return;

    var stack = getUndoStack(path);
    stack.undo.push(content);
    if (stack.undo.length > 200) stack.undo.shift();
    stack.redo = [];

    state.undo.lastContent = content;
    state.undo.lastPushTime = now;
}

function undo() {
    var path = state.activeFile;
    if (!path) return;
    var stack = getUndoStack(path);
    if (stack.undo.length === 0) return;

    var currentContent = state.fileContents.get(path) || '';
    stack.redo.push(currentContent);

    var prev = stack.undo.pop();
    state.fileContents.set(path, prev);
    state.undo.lastContent = prev;

    var editor = document.getElementById('code-editor');
    setEditorContent(prev);
    editor.dataset.path = path;
    state.modifiedFiles.add(path);
    renderTabs();
    updateLineNumbers();
    scheduleHighlight();
}

function redo() {
    var path = state.activeFile;
    if (!path) return;
    var stack = getUndoStack(path);
    if (stack.redo.length === 0) return;

    var currentContent = state.fileContents.get(path) || '';
    stack.undo.push(currentContent);

    var next = stack.redo.pop();
    state.fileContents.set(path, next);
    state.undo.lastContent = next;

    var editor = document.getElementById('code-editor');
    setEditorContent(next);
    editor.dataset.path = path;
    state.modifiedFiles.add(path);
    renderTabs();
    updateLineNumbers();
    scheduleHighlight();
}


function showEditor() {
    document.getElementById('editor-placeholder').classList.add('hidden');
    document.getElementById('editor-content').classList.remove('hidden');
}

function showPlaceholder() {
    document.getElementById('editor-placeholder').classList.remove('hidden');
    document.getElementById('editor-content').classList.add('hidden');
}

async function onEditorInput() {
    if (inlineDiffState.active) {
        clearInlineDiffs();
    }

    var editor = document.getElementById('code-editor');
    var path = editor.dataset.path;

    if (path) {
        pushUndo(path);
        state.fileContents.set(path, getEditorContent());
        state.modifiedFiles.add(path);
        renderTabs();
    }

    updateLineNumbers();
    updateCursorPosition();

    if (state.markdownPreviewActive) {
        refreshMarkdownPreview();
    } else {
        scheduleHighlight();
    }
}

var acState = { items: [], selectedIndex: -1, range: null };

function acShow(items, range) {
    acState.items = items;
    acState.selectedIndex = 0;
    acState.range = range;
    var popup = document.getElementById('autocomplete-popup');
    popup.innerHTML = '';
    for (var i = 0; i < items.length; i++) {
        var item = document.createElement('div');
        item.className = 'autocomplete-item' + (i === 0 ? ' selected' : '');
        item.dataset.index = i;
        var kind = document.createElement('span');
        kind.className = 'ac-kind';
        kind.textContent = items[i].kind || '?';
        var label = document.createElement('span');
        label.className = 'ac-label';
        label.textContent = items[i].label || items[i].prefix;
        item.appendChild(kind);
        item.appendChild(label);
        item.addEventListener('click', function() { acAccept(parseInt(this.dataset.index)); });
        item.addEventListener('mouseenter', function() {
            acState.items.forEach(function(_, j) {
                popup.children[j].classList.toggle('selected', j === parseInt(this.dataset.index));
            }.bind(this));
            acState.selectedIndex = parseInt(this.dataset.index);
        });
        popup.appendChild(item);
    }
    popup.classList.remove('hidden');
}

function acHide() {
    var popup = document.getElementById('autocomplete-popup');
    popup.classList.add('hidden');
    popup.innerHTML = '';
    acState = { items: [], selectedIndex: -1, range: null };
}

function acAccept(index) {
    var item = acState.items[index];
    if (!item) return;
    if (item.type === 'snippet') {
        var editor = document.getElementById('code-editor');
        var before = (editor.textContent || '').substring(0, acState.range.start);
        var after = (editor.textContent || '').substring(acState.range.end);
        var expanded = item.body.replace(/\$\{(\d+):([^}]*)\}|\$(\d+)|\${\s*(\d+)\s*}/g, function(_, n, v, n2, n3) { return v || ''; });
        var full = before + expanded + after;
        editor.textContent = full;
        var pos = before.length + expanded.length;
        setEditorCursor(pos);
    } else {
        var editor = document.getElementById('code-editor');
        var before = (editor.textContent || '').substring(0, acState.range.start);
        var after = (editor.textContent || '').substring(acState.range.end);
        editor.textContent = before + item.prefix + after;
        setEditorCursor(before.length + item.prefix.length);
    }
    acHide();
    onEditorInput();
}

function onEditorKeydown(e) {
    var popup = document.getElementById('autocomplete-popup');
    var acVisible = !popup.classList.contains('hidden') && acState.items.length > 0;

    if (acVisible && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
        e.preventDefault();
        var dir = e.key === 'ArrowDown' ? 1 : -1;
        acState.selectedIndex = Math.max(0, Math.min(acState.items.length - 1, acState.selectedIndex + dir));
        for (var i = 0; i < popup.children.length; i++) {
            popup.children[i].classList.toggle('selected', i === acState.selectedIndex);
        }
        return;
    }

    if (acVisible && (e.key === 'Enter' || e.key === 'Tab')) {
        e.preventDefault();
        if (acState.selectedIndex >= 0) {
            acAccept(acState.selectedIndex);
        }
        return;
    }

    if (e.key === 'Escape') {
        if (acVisible) { acHide(); e.preventDefault(); return; }
    }

    if (e.key === 'Tab' && !acVisible) {
        e.preventDefault();
        var sel = getEditorSelection();
        var editor = document.getElementById('code-editor');
        var content = editor.textContent || '';
        if (sel.start === sel.end) {
            var before = content.substring(0, sel.start);
            var match = before.match(/[a-zA-Z_0-9]*$/);
            if (match && match[0].length > 0) {
                var word = match[0];
                var lang = window.getCompletions ? (function() {
                    var l = window.getLangForSnippets ? window.getLangForSnippets() : null;
                    return l;
                })() : null;
                if (lang) {
                    var snippets = window.getSnippetsForLang ? window.getSnippetsForLang(lang) : [];
                    for (var si = 0; si < snippets.length; si++) {
                        if (snippets[si].prefix === word) {
                            var wStart = sel.start - word.length;
                            var expanded = snippets[si].body.replace(/\$\{(\d+):([^}]*)\}|\$(\d+)|\${\s*(\d+)\s*}/g, function(_, n, v) { return v || ''; });
                            editor.textContent = content.substring(0, wStart) + expanded + content.substring(sel.start);
                            setEditorCursor(wStart + expanded.length);
                            onEditorInput();
                            return;
                        }
                    }
                }
            }
        }
        setEditorContent(content.substring(0, sel.start) + '    ' + content.substring(sel.end));
        setEditorSelection(sel.start + 4, sel.start + 4);
        onEditorInput();
    }

    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        saveCurrentFile();
    }

    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && e.key === 'z') {
        e.preventDefault();
        undo();
    }
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === 'z') {
        e.preventDefault();
        redo();
    }
}

var hoverTooltipTimer = null;

function onEditorHover(e) {
    var tooltip = document.getElementById('editor-hover-tooltip');
    if (!tooltip) return;

    clearTimeout(hoverTooltipTimer);
    tooltip.classList.add('hidden');

    hoverTooltipTimer = setTimeout(function() {
        var editor = document.getElementById('code-editor');
        var path = editor.dataset.path;
        if (!path) return;

        var sel = getEditorSelection();
        var text = editor.textContent || '';
        if (text.length === 0) return;
        var offset = sel.start;
        var before = text.substring(0, offset);
        var wordStart = before.search(/[a-zA-Z_][a-zA-Z0-9_]*$/);
        if (wordStart < 0) return;
        var word = before.substring(wordStart);

        if (typeof findSymbol === 'function') {
            var results = findSymbol(word);
            if (results.length > 0) {
                var info = '**' + word + '**\n\n';
                for (var i = 0; i < Math.min(results.length, 5); i++) {
                    var r = results[i];
                    info += '- defined in `' + r.path + ':' + r.symbol.line + '` (' + r.symbol.kind + ')\n';
                }
                tooltip.innerHTML = info.replace(/\n/g, '<br>');
                tooltip.style.top = e.clientY + 16 + 'px';
                tooltip.style.left = e.clientX + 'px';
                tooltip.classList.remove('hidden');
                return;
            }
        }

        tooltip.classList.add('hidden');
    }, 300);
}

function onEditorKeyup(e) {
    if (e.key === 'Escape') return;
    if (e.key === 'Enter' || e.key === 'Tab') return;
    if (e.key.length !== 1 && e.key !== 'Backspace') return;
    var editor = document.getElementById('code-editor');
    var sel = getEditorSelection();
    if (sel.start !== sel.end) { acHide(); return; }
    var text = editor.textContent || '';
    if (text.length === 0) { acHide(); return; }
    var before = text.substring(0, sel.start);
    var wordMatch = before.match(/[a-zA-Z_0-9]+$/);
    if (!wordMatch || wordMatch[0].length < 1) { acHide(); return; }
    var word = wordMatch[0];
    var cursorOffset = sel.start;
    var wordStart = cursorOffset - word.length;
    if (window.getCompletions) {
        var results = window.getCompletions(editor, cursorOffset);
        if (results.length > 0) {
            acShow(results, { start: wordStart, end: cursorOffset });
            var popup = document.getElementById('autocomplete-popup');
            var editorRect = editor.getBoundingClientRect();
            var lineHeight = parseInt(getComputedStyle(editor).lineHeight) || 20;
            var textBefore = text.substring(0, cursorOffset);
            var lines = textBefore.split('\n');
            var currentLine = lines.length - 1;
            var lineStart = textBefore.lastIndexOf('\n') + 1;
            var charPos = cursorOffset - lineStart;
            var charWidth = 8;
            var top = editorRect.top + (currentLine + 1) * lineHeight;
            var left = editorRect.left + charPos * charWidth;
            if (top + 300 > window.innerHeight) top = editorRect.top + (currentLine) * lineHeight - 300;
            if (left + 250 > window.innerWidth) left = window.innerWidth - 260;
            if (left < 0) left = 0;
            popup.style.top = top + 'px';
            popup.style.left = left + 'px';
        } else {
            acHide();
        }
    }
}

async function saveCurrentFile() {
    var editor = document.getElementById('code-editor');
    var path = editor.dataset.path;
    if (!path) return;

    try {
        await invoke('write_file', { path: path, content: getEditorContent() });
        state.modifiedFiles.delete(path);
        renderTabs();
    } catch (err) {
        console.error('Error guardando archivo:', err);
    }
}

function updateLineNumbers() {
    var lineNumbers = document.getElementById('line-numbers');
    var content = getEditorContent();
    var lines = content.split('\n').length;
    var html = '';
    for (var i = 1; i <= lines; i++) {
        html += '<div class="line-number">' + i + '</div>';
    }
    lineNumbers.innerHTML = html;
}

function syncLineNumbersScroll() {
    var editor = document.getElementById('code-editor');
    var lineNumbers = document.getElementById('line-numbers');
    lineNumbers.scrollTop = editor.scrollTop;

    var pre = document.querySelector('.highlights-pre');
    if (pre) {
        pre.style.transform = 'translate(' + (-editor.scrollLeft) + 'px, ' + (-editor.scrollTop) + 'px)';
    }
}

function updateExploreButton() {
    var btn = document.getElementById('explore-btn');
    var span = btn.querySelector('span');
    span.textContent = state.workspacePath ? 'Explore' : 'Select Workspace';
    var headerName = document.getElementById('explore-workspace-name');
    if (headerName) {
        headerName.textContent = state.workspacePath
            ? state.workspacePath.split('/').pop()
            : 'Workspace';
    }
}

function resetWorkspaceState() {
    state.openFiles.forEach(function(f) { state.fileContents.delete(f.path); });
    state.openFiles = [];
    state.activeFile = null;
    state.modifiedFiles.clear();
    clearHighlights();
    showPlaceholder();
    renderTabs();
    updateGitStatus();
    newSession();
    saveTabsState();
}

function updateCursorPosition() {
    var sel = getEditorSelection();
    var text = getEditorContent().substring(0, sel.start);
    var lines = text.split('\n');
    var line = lines.length;
    var col = lines[lines.length - 1].length + 1;
    document.getElementById('cursor-position').textContent = 'Ln ' + line + ', Col ' + col;
}


function toggleAgentsPanel() {
    var panel = document.getElementById('agents-panel');
    state.agentsPanelVisible = !state.agentsPanelVisible;
    panel.classList.toggle('collapsed', !state.agentsPanelVisible);
    if (state.agentsPanelVisible) {
        adjustAgentsPanelWidth();
    }
}

function setAgentMode(mode) {
    state.agentMode = mode;
    
    document.getElementById('current-mode-label').textContent = 
        mode.charAt(0).toUpperCase() + mode.slice(1);
    
    document.querySelectorAll('.mode-option').forEach(function(opt) {
        opt.classList.toggle('active', opt.dataset.mode === mode);
    });
    
    document.querySelectorAll('.agent-mode').forEach(function(m) {
        m.classList.add('hidden');
    });
    document.getElementById('mode-' + mode).classList.remove('hidden');
    
    var input = document.getElementById('agent-input');
    switch(mode) {
        case 'plan':
            input.placeholder = 'Describe what you want to implement...';
            break;
        case 'build':
            input.placeholder = 'Build commands or feedback...';
            break;
        case 'ask':
            input.placeholder = 'Ask about the code...';
            break;
    }
    
    // Update status bar mode indicator
    var modeIndicator = document.getElementById('mode-indicator');
    var modeLabel = document.getElementById('mode-label');
    if (modeIndicator && modeLabel) {
        modeIndicator.classList.remove('hidden');
        modeLabel.textContent = mode.charAt(0).toUpperCase() + mode.slice(1);
    }
    
    // Focus input after mode switch
    if (input) {
        input.focus();
    }
}

function setupModeSelector() {
    var toggle = document.getElementById('mode-toggle');
    var dropdown = document.getElementById('mode-dropdown');
    
    toggle.addEventListener('click', function(e) {
        e.stopPropagation();
        dropdown.classList.toggle('hidden');
    });
    
    document.querySelectorAll('.mode-option').forEach(function(option) {
        option.addEventListener('click', function() {
            setAgentMode(option.dataset.mode);
            dropdown.classList.add('hidden');
        });
    });
    
    document.addEventListener('click', function(e) {
        if (!dropdown.contains(e.target) && !toggle.contains(e.target)) {
            dropdown.classList.add('hidden');
        }
    });
}

function setupAutoResize() {
    var input = document.getElementById('agent-input');
    input.addEventListener('input', function() {
        this.style.height = 'auto';
        this.style.height = Math.min(this.scrollHeight, 150) + 'px';
    });
}

function toggleToolLog() {
    state.toolLogVisible = !state.toolLogVisible;
    var subpanel = document.getElementById('tool-log-subpanel');
    var emptyState = document.getElementById('tool-log-empty');
    var entries = document.getElementById('tool-log-entries');
    var toggleBtn = document.getElementById('tool-log-toggle-btn');
    var resizeHandle = document.getElementById('tool-log-resize-handle');
    
    if (state.toolLogVisible) {
        subpanel.classList.remove('collapsed');
        subpanel.style.width = '300px';
        if (resizeHandle) resizeHandle.style.display = 'block';
        if (state.toolCalls.length > 0) {
            emptyState.style.display = 'none';
            entries.style.display = 'block';
        }
        if (toggleBtn) toggleBtn.classList.add('active');
    } else {
        subpanel.classList.add('collapsed');
        subpanel.style.width = '0';
        if (resizeHandle) resizeHandle.style.display = 'none';
        if (toggleBtn) toggleBtn.classList.remove('active');
    }
    adjustAgentsPanelWidth();
}

function adjustAgentsPanelWidth() {
    var panel = document.getElementById('agents-panel');
    var subpanel = document.getElementById('tool-log-subpanel');
    var container = document.getElementById('main-container');
    
    if (!panel || !container) return;
    
    var containerRect = container.getBoundingClientRect();
    var minWidth = 300;
    var maxWidth = 800;
    
    if (state.toolLogVisible && state.toolCalls.length > 0) {
        var newWidth = Math.min(maxWidth, Math.max(minWidth, containerRect.width * 0.5));
        panel.style.width = newWidth + 'px';
    } else {
        var currentPercent = (panel.offsetWidth / containerRect.width) * 100;
        var newWidth = Math.min(maxWidth, Math.max(minWidth, containerRect.width * (currentPercent / 100)));
        panel.style.width = newWidth + 'px';
    }
}

function toggleDiffPopup(e) {
    if (e) e.stopPropagation();
    var popup = document.getElementById('diff-popup');
    if (!popup) return;
    popup.classList.remove('hidden');
    renderDiffPopupList();
}

function closeDiffPopup() {
    var popup = document.getElementById('diff-popup');
    if (popup) popup.classList.add('hidden');
}

function renderDiffPopupList() {
    var list = document.getElementById('diff-popup-list');
    var countEl = document.getElementById('diff-popup-count');
    if (!list) return;
    
    var diffs = Object.values(state.pendingDiffs || {});
    if (countEl) countEl.textContent = '(' + diffs.length + ')';
    
    if (diffs.length === 0) {
        list.innerHTML = '<div class="diff-popup-empty">No pending diffs</div>';
        return;
    }
    
    list.innerHTML = diffs.map(function(diff) {
        var addStr = diff.stats.additions > 0 ? '+' + diff.stats.additions : '';
        var delStr = diff.stats.deletions > 0 ? '-' + diff.stats.deletions : '';
        return '<div class="diff-popup-item" onclick="openDiff(\'' + diff.filePath + '\')">' +
            '<span class="diff-popup-file">' + escapeHtml(diff.filePath) + '</span>' +
            '<div class="diff-popup-stats">' +
                (addStr ? '<span class="diff-popup-add">' + addStr + '</span>' : '') +
                (delStr ? '<span class="diff-popup-del">' + delStr + '</span>' : '') +
            '</div>' +
        '</div>';
    }).join('');
}

function openDiff(filePath) {
    closeDiffPopup();
    if (state.pendingDiffs && state.pendingDiffs[filePath]) {
        var diff = state.pendingDiffs[filePath];
        var proposal = {
            id: filePath,
            file_path: filePath,
            kind: 'modify',
            before: diff.oldContent,
            after: diff.newContent,
            unified_diff: diff.unifiedDiff
        };
        if (state.activeFile === filePath) {
            showInlineDiff(proposal);
        } else {
            openFile(filePath, filePath.split('/').pop()).then(function() {
                setTimeout(function() { showInlineDiff(proposal); }, 100);
            });
        }
    }
}

function acceptAllDiffs() {
    if (!state.pendingDiffs) return;
    Object.keys(state.pendingDiffs).forEach(function(filePath) {
        applyDiff(filePath);
    });
    state.pendingDiffs = {};
    updateDiffBadge();
    closeDiffPopup();
}

function rejectAllDiffs() {
    state.pendingDiffs = {};
    updateDiffBadge();
    closeDiffPopup();
    // Refresh editor to remove diff highlights
    if (state.activeFile) {
        scheduleHighlight();
    }
}

function applyDiff(filePath) {
    var diff = state.pendingDiffs[filePath];
    if (!diff) return;
    
    // Apply the diff content to the file
    state.fileContents.set(filePath, diff.newContent);
    state.modifiedFiles.add(filePath);
    
    // Remove from pending
    delete state.pendingDiffs[filePath];
    updateDiffBadge();
    
    // Update editor if this file is active
    if (state.activeFile === filePath) {
        var editor = document.getElementById('code-editor');
        if (editor) {
            setEditorContent(diff.newContent);
            onEditorInput();
        }
    }
    
    renderTabs();
}

function updateDiffBadge() {
    var btn = document.getElementById('diff-toggle-btn');
    var badge = document.getElementById('diff-count-badge');
    if (!btn || !badge) return;
    
    var count = Object.keys(state.pendingDiffs || {}).length;
    if (count > 0) {
        btn.classList.remove('hidden');
        badge.classList.remove('hidden');
        badge.textContent = count;
    } else {
        badge.classList.add('hidden');
        // Keep button visible if user wants to access it
    }
}

function addPendingDiff(filePath, oldContent, newContent, unifiedDiff) {
    if (!state.pendingDiffs) state.pendingDiffs = {};
    
    // Calculate stats
    var additions = 0;
    var deletions = 0;
    if (unifiedDiff) {
        var lines = unifiedDiff.split('\n');
        lines.forEach(function(line) {
            if (line.startsWith('+') && !line.startsWith('+++')) additions++;
            if (line.startsWith('-') && !line.startsWith('---')) deletions++;
        });
    }
    
    state.pendingDiffs[filePath] = {
        filePath: filePath,
        oldContent: oldContent,
        newContent: newContent,
        unifiedDiff: unifiedDiff,
        stats: { additions: additions, deletions: deletions },
        timestamp: Date.now()
    };
    
    updateDiffBadge();
}

var COMMANDS = [
    { id: 'new-session', label: 'New Session', desc: 'Start a fresh conversation', icon: '⟳', shortcut: 'Ctrl+Shift+N', action: function() { newSession(); } },
    { id: 'toggle-agents', label: 'Toggle Agents Panel', desc: 'Show or hide the agent panel', icon: '⊞', shortcut: 'Ctrl+B', action: function() { toggleAgentsPanel(); } },
    { id: 'toggle-tool-log', label: 'Toggle Tool Log', desc: 'Show or hide the tool execution log', icon: '📋', shortcut: 'Ctrl+Shift+L', action: function() { toggleToolLog(); } },
    { id: 'show-diffs', label: 'Show Pending Diffs', desc: 'Review pending code changes', icon: '📑', shortcut: 'Ctrl+D', action: function() { toggleDiffPopup(); } },
    { id: 'mode-plan', label: 'Switch to Plan Mode', desc: 'Create implementation plans', icon: '📝', shortcut: '', action: function() { setAgentMode('plan'); } },
    { id: 'mode-build', label: 'Switch to Build Mode', desc: 'Execute build tasks', icon: '🔨', shortcut: '', action: function() { setAgentMode('build'); } },
    { id: 'mode-ask', label: 'Switch to Ask Mode', desc: 'Ask questions about the code', icon: '❓', shortcut: '', action: function() { setAgentMode('ask'); } },
    { id: 'open-settings', label: 'Open Settings', desc: 'Configure providers and preferences', icon: '⚙️', shortcut: 'Ctrl+,', action: function() { toggleSettings(); } },
    { id: 'change-workspace', label: 'Change Workspace', desc: 'Open a different workspace folder', icon: '📁', shortcut: 'Ctrl+Shift+O', action: function() { showWorkspacePicker(); } },
    { id: 'save-file', label: 'Save File', desc: 'Save the current file', icon: '💾', shortcut: 'Ctrl+S', action: function() { saveCurrentFile(); } },
    { id: 'close-file', label: 'Close File', desc: 'Close the current file', icon: '✕', shortcut: 'Ctrl+W', action: function() { closeFile(state.activeFile); } },
    { id: 'focus-input', label: 'Focus Chat Input', desc: 'Focus the agent chat input', icon: '⌨️', shortcut: 'Ctrl+Shift+I', action: function() { document.getElementById('agent-input').focus(); } },
    { id: 'clear-chat', label: 'Clear Chat', desc: 'Clear the conversation history', icon: '🗑️', shortcut: '', action: function() { clearChat(); } },
    { id: 'toggle-markdown', label: 'Toggle Markdown Preview', desc: 'Preview markdown files', icon: '👁️', shortcut: '', action: function() { toggleMarkdownPreview(); } },
];

var commandPaletteSelected = 0;
var commandPaletteFiltered = [];

function openCommandPalette() {
    var palette = document.getElementById('command-palette');
    var input = document.getElementById('command-palette-input');
    if (!palette || !input) return;
    
    palette.classList.remove('hidden');
    input.value = '';
    input.focus();
    commandPaletteSelected = 0;
    commandPaletteFiltered = COMMANDS.slice();
    renderCommandPaletteList();
}

function closeCommandPalette() {
    var palette = document.getElementById('command-palette');
    if (palette) palette.classList.add('hidden');
}

function renderCommandPaletteList() {
    var list = document.getElementById('command-palette-list');
    if (!list) return;
    
    if (commandPaletteFiltered.length === 0) {
        list.innerHTML = '<div class="command-palette-no-results">No commands found</div>';
        return;
    }
    
    list.innerHTML = commandPaletteFiltered.map(function(cmd, index) {
        var selected = index === commandPaletteSelected ? ' selected' : '';
        var shortcutHtml = cmd.shortcut ? '<kbd>' + escapeHtml(cmd.shortcut) + '</kbd>' : '';
        return '<div class="command-palette-item' + selected + '" data-index="' + index + '" onclick="executeCommand(' + index + ')">' +
            '<span class="command-palette-item-icon">' + cmd.icon + '</span>' +
            '<div class="command-palette-item-text">' +
                '<div class="command-palette-item-label">' + escapeHtml(cmd.label) + '</div>' +
                '<div class="command-palette-item-desc">' + escapeHtml(cmd.desc) + '</div>' +
            '</div>' +
            '<div class="command-palette-item-shortcut">' + shortcutHtml + '</div>' +
        '</div>';
    }).join('');
    
    // Scroll selected into view
    var selectedEl = list.querySelector('.command-palette-item.selected');
    if (selectedEl) {
        selectedEl.scrollIntoView({ block: 'nearest' });
    }
}

function executeCommand(index) {
    var cmd = commandPaletteFiltered[index];
    if (!cmd) return;
    closeCommandPalette();
    cmd.action();
}

function filterCommands(query) {
    if (!query) {
        commandPaletteFiltered = COMMANDS.slice();
        return;
    }
    
    var lowerQuery = query.toLowerCase();
    commandPaletteFiltered = COMMANDS.filter(function(cmd) {
        return cmd.label.toLowerCase().indexOf(lowerQuery) !== -1 ||
               cmd.desc.toLowerCase().indexOf(lowerQuery) !== -1 ||
               cmd.id.toLowerCase().indexOf(lowerQuery) !== -1;
    });
    
    commandPaletteSelected = 0;
}

function clearChat() {
    state.conversationHistory = [];
    var container = document.getElementById('chat-messages');
    if (container) container.innerHTML = '';
}

function showToast(message, type) {
    type = type || 'info';
    var container = document.getElementById('toast-container');
    if (!container) return;
    
    var toast = document.createElement('div');
    toast.className = 'toast-notification toast-' + type;
    toast.textContent = message;
    
    container.appendChild(toast);
    
    setTimeout(function() {
        toast.classList.add('removing');
        setTimeout(function() {
            if (toast.parentNode) toast.parentNode.removeChild(toast);
        }, 300);
    }, 3000);
}

function filterToolLog() {
    var searchInput = document.getElementById('tool-log-search');
    var filterSelect = document.getElementById('tool-log-filter');
    var entries = document.getElementById('tool-log-entries');
    if (!entries) return;
    
    var searchTerm = searchInput ? searchInput.value.toLowerCase() : '';
    var toolFilter = filterSelect ? filterSelect.value : '';
    
    var allEntries = entries.querySelectorAll('.tool-log-entry');
    allEntries.forEach(function(entry) {
        var toolName = entry.dataset.toolName || '';
        var text = entry.textContent.toLowerCase();
        var matchesSearch = !searchTerm || text.indexOf(searchTerm) !== -1;
        var matchesTool = !toolFilter || toolName === toolFilter;
        
        if (matchesSearch && matchesTool) {
            entry.style.display = '';
        } else {
            entry.style.display = 'none';
        }
    });
}





function updateTokenDisplay() {
    updateToolLogInfo();
}

function updateToolLogInfo() {
    var modelEl = document.getElementById('tl-model');
    var sessionEl = document.getElementById('tl-session');
    var tokensEl = document.getElementById('tl-tokens');
    var contextEl = document.getElementById('tl-context');

    if (modelEl) {
        var provider = state.providers.find(function(p) { return p.id === state.selectedProviderId; });
        modelEl.textContent = (provider ? provider.name : state.selectedProviderId) + ' / ' + (state.selectedModelId || '--');
    }

    if (sessionEl) {
        sessionEl.textContent = state.sessionTitle || (state.currentRunId ? state.currentRunId.substring(0, 20) : '--');
    }

    if (tokensEl) {
        tokensEl.textContent = 'Session: ' + state.tokenBreakdown.total.toLocaleString()
            + ' tokens (In: ' + state.tokenBreakdown.input.toLocaleString()
            + ' | Out: ' + state.tokenBreakdown.output.toLocaleString() + ')';
    }

    if (contextEl) {
        var used = state.lastRequestTokens || state.tokenBreakdown.total;
        if (state.modelContextInfo && state.modelContextInfo.max_context_tokens > 0) {
            var total = state.modelContextInfo.max_context_tokens;
            if (used > total) {
                contextEl.textContent = 'Last run: ' + used.toLocaleString() + ' / ' + total.toLocaleString() + ' (exceeded)';
                contextEl.style.color = 'var(--error)';
            } else {
                var pct = Math.round((used / total) * 100);
                contextEl.textContent = 'Last run: ' + used.toLocaleString() + ' / ' + total.toLocaleString() + ' (' + pct + '%)';
                contextEl.style.color = '';
            }
        } else {
            contextEl.textContent = 'Last run: ' + used.toLocaleString() + ' / --';
            contextEl.style.color = '';
        }
    }
}

async function fetchModelContextInfo() {
    if (!state.selectedProviderId || !state.selectedModelId) return;
    try {
        var info = await invoke('get_model_context_info', {
            providerId: state.selectedProviderId,
            modelId: state.selectedModelId
        });
        state.modelContextInfo = info;
        updateToolLogInfo();
    } catch (e) {
        // ignore
    }
}

function nowSeconds() {
    return Math.floor(Date.now() / 1000);
}

async function loadPersistedModelCache() {
    try {
        var cache = await invoke('load_cached_models');
        if (cache) {
            state.providerModelCache = cache;
        }
    } catch (e) {
        // ignore
    }
}

function savePersistedModelCache() {
    invoke('save_cached_models', { cache: state.providerModelCache }).catch(function(e) {
        console.warn('Failed to save model cache:', e);
    });
}




function setupModelSelector() {
    var btn = document.getElementById('model-selector-btn');
    var dropdown = document.getElementById('model-dropdown');
    
    btn.addEventListener('click', function(e) {
        e.stopPropagation();
        dropdown.classList.toggle('hidden');
        if (!dropdown.classList.contains('hidden')) {
            renderModelDropdown();
        }
    });
    
    document.addEventListener('click', function(e) {
        if (!dropdown.contains(e.target) && !btn.contains(e.target)) {
            dropdown.classList.add('hidden');
        }
    });
}

async function loadModelSelector() {
    var keylessProviders = state.providers.filter(function(p) { return p.is_keyless; });
    var otherProviders = state.providers.filter(function(p) { return !p.is_keyless; });
    var sortedProviders = keylessProviders.concat(otherProviders);
    
    for (var provider of sortedProviders) {
        if (provider.is_keyless) {
            if (!state.selectedProviderId) {
                state.selectedProviderId = provider.id;
                state.selectedModelId = provider.default_models[0] || null;
            }
        } else if (!provider.api_key_required || state.providerModels[provider.id]) {
            if (!state.selectedProviderId) {
                state.selectedProviderId = provider.id;
                state.selectedModelId = provider.default_models[0] || null;
            }
        }
    }

    state.providers = sortedProviders;
    updateModelSelectorDisplay();
    updateReasoningSelectorVisibility();
}

function updateModelSelectorDisplay() {
    var nameEl = document.getElementById('current-model-name');
    if (state.selectedProviderId && state.selectedModelId) {
        var provider = state.providers.find(function(p) { return p.id === state.selectedProviderId; });
        nameEl.textContent = (provider ? provider.name : state.selectedProviderId) + ' / ' + state.selectedModelId;
    } else {
        nameEl.textContent = 'No model selected';
    }
    updateReasoningSelectorVisibility();
}

function updateReasoningSelectorVisibility() {
    var el = document.querySelector('.reasoning-selector');
    if (!el) return;
    el.classList.remove('hidden');
    state.reasoningEffort = document.getElementById('reasoning-effort').value || 'low';
}

async function renderModelDropdown() {
    var dropdown = document.getElementById('model-dropdown');
    dropdown.innerHTML = '';

    var availableGroup = document.createElement('div');
    availableGroup.className = 'model-dropdown-group';
    var availableHeader = document.createElement('div');
    availableHeader.className = 'model-dropdown-group-header';
    availableHeader.textContent = 'Available';
    availableGroup.appendChild(availableHeader);

    var keylessProviders = state.providers.filter(function(p) { return p.is_keyless; });
    var otherProviders = state.providers.filter(function(p) { return !p.is_keyless; });
    var sortedProviders = keylessProviders.concat(otherProviders);

    var providerModels = [];
    var providerApiKeyStatus = {};

    for (var i = 0; i < sortedProviders.length; i++) {
        var provider = sortedProviders[i];

        var hasApiKey = !provider.api_key_required;
        if (provider.api_key_required) {
            try {
                var config = await invoke('get_provider_config_cmd', { providerId: provider.id });
                hasApiKey = !!(config && config.api_key);
            } catch (e) {}
        }

        providerApiKeyStatus[provider.id] = hasApiKey;

        if (provider.api_key_required && !hasApiKey) continue;

        var models = [];
        if (provider.models_endpoint && hasApiKey) {
            if (state.providerModels[provider.id]) {
                models = state.providerModels[provider.id];
            } else if (state.providerModelCache[provider.id]) {
                var entry = state.providerModelCache[provider.id];
                var CACHE_TTL = 3600;
                if (nowSeconds() - entry.cached_at < CACHE_TTL) {
                    models = entry.models.map(function(m) { return m.id || m.name; });
                    state.providerModels[provider.id] = models;
                }
            }
            if (models.length === 0) {
                try {
                    var fetched = await invoke('fetch_provider_models', { providerId: provider.id });
                    if (fetched && fetched.length > 0) {
                        models = fetched.map(function(m) { return m.id || m.name; });
                        state.providerModels[provider.id] = models;
                        state.providerModelCache[provider.id] = { models: fetched, cached_at: nowSeconds() };
                        savePersistedModelCache();
                    }
                } catch (e) {
                    models = provider.default_models || [];
                }
            }
        } else {
            models = provider.default_models || [];
        }

        if (models.length > 0) {
            providerModels.push({ provider: provider, models: models });
        }
    }

    var modelCounts = {};
    providerModels.forEach(function(entry) {
        entry.models.forEach(function(m) {
            modelCounts[m] = (modelCounts[m] || 0) + 1;
        });
    });

    providerModels.forEach(function(entry) {
        var provider = entry.provider;

        var providerHeader = document.createElement('div');
        providerHeader.className = 'model-dropdown-provider-header';
        providerHeader.textContent = provider.name + (provider.is_keyless ? ' (Free)' : '');
        availableGroup.appendChild(providerHeader);

        entry.models.forEach(function(modelId) {
            var item = document.createElement('div');
            item.className = 'model-dropdown-item';
            if (modelId === state.selectedModelId && provider.id === state.selectedProviderId) {
                item.classList.add('active');
            }

            var nameSpan = document.createElement('span');
            nameSpan.className = 'model-id';
            nameSpan.textContent = modelCounts[modelId] > 1 ? modelId + ' (' + provider.name + ')' : modelId;
            item.appendChild(nameSpan);

            item.addEventListener('click', (function(provId, mId) {
                return function() {
                    state.selectedProviderId = provId;
                    state.selectedModelId = mId;
                    updateModelSelectorDisplay();
                    updateReasoningSelectorVisibility();
                    dropdown.classList.add('hidden');
                    fetchModelContextInfo();
                };
            })(provider.id, modelId));

            availableGroup.appendChild(item);
        });
    });

    if (availableGroup.children.length > 1) {
        dropdown.appendChild(availableGroup);
    }
}


async function sendAskMessage() {
    var input = document.getElementById('agent-input');
    var message = input.value.trim();
    if (!message) return;
    if (state.isLoadingCompletion) return;

    var mode = state.agentMode;
    var runId = 'run_' + Date.now() + '_' + Math.random().toString(36).slice(2, 8);

    var projectSummary = '';
    try {
        if (typeof buildProjectSummary === 'function') {
            projectSummary = '\n\n## Project Index\n' + buildProjectSummary();
        }
    } catch (e) {}

    var enrichedMessage = message + projectSummary;

    addChatMessage('user', message, true);

    pushHistorySnapshot();

    state.conversationHistory.push({
        role: 'user',
        content: message,
        mode: mode,
        provider: state.selectedProviderId,
        model: state.selectedModelId,
        timestamp: Date.now()
    });

    state.conversationHistory = truncateConversation(state.conversationHistory);

    input.value = '';
    input.style.height = 'auto';

    state.isLoadingCompletion = true;
    state.currentRunId = runId;
    state.hasStartedToolCall = false;
    state._hasContentBeforeToolCall = false;
    showStopButton();
    var thinkingEl = addThinkingIndicator(mode);

    try {
        var response;
        if (!state.selectedProviderId || !state.selectedModelId) {
            response = 'No model selected. Please select a model in the Agent panel header.';
        } else if (mode === 'plan') {
            var planResult = await invoke('generate_plan', {
                runId: runId,
                description: enrichedMessage,
                providerId: state.selectedProviderId,
                modelId: state.selectedModelId,
                reasoningEffort: state.reasoningEffort
            });

            if (planResult && planResult.file_path) {
                renderPlanCard(planResult);
                response = '[Plan: ' + (planResult.title || 'Untitled') + '](' + planResult.file_path + ')';
                state.conversationHistory.push({
                    role: 'assistant',
                    content: response,
                    mode: 'plan',
                    provider: state.selectedProviderId,
                    model: state.selectedModelId,
                    timestamp: Date.now()
                });
            } else {
                response = 'Plan generated but no file was created.';
            }
        } else if (mode === 'build') {
            var buildResult = await invoke('run_build_agent', {
                runId: runId,
                planDescription: enrichedMessage,
                planAnnotations: null,
                providerId: state.selectedProviderId,
                modelId: state.selectedModelId,
                reasoningEffort: state.reasoningEffort
            });
            response = 'Build complete. ' + (buildResult.proposals ? buildResult.proposals.length : 0) + ' changes proposed.';
            if (buildResult.proposals && buildResult.proposals.length > 0) {
                showDiffProposals(buildResult.proposals);
            }
        } else {
            var systemPrompt = buildSystemPrompt(mode);
            var result = await invoke('send_completion', {
                runId: runId,
                providerId: state.selectedProviderId,
                modelId: state.selectedModelId,
                reasoningEffort: state.reasoningEffort,
                messages: state.conversationHistory,
                systemPrompt: systemPrompt,
            });
            response = result.content || result;
            if (result.tokens_used) {
                state.tokenUsage.session += result.tokens_used;
                state.tokenUsage.total += result.tokens_used;
                updateTokenDisplay();
            }
        }

        if (response) {
            addChatMessage('assistant', response);
            state.conversationHistory.push({
                role: 'assistant',
                content: response,
                mode: mode,
                provider: state.selectedProviderId,
                model: state.selectedModelId,
                timestamp: Date.now()
            });
        }
    } catch (err) {
        var errorContent = 'Error: ' + err;
        addChatMessage('assistant', errorContent);
    } finally {
        removeThinkingIndicator(thinkingEl);
        hideStopButton();
        state.isLoadingCompletion = false;
        state.currentRunId = null;
    }
}

function buildSystemPrompt(mode) {
    var base = 'You are TundraCode, an AI coding assistant. ';
    var announce = ' Before each action (reading files, searching, or writing code), briefly explain what you are about to do and why. This helps the user understand your reasoning in real-time.';
    switch(mode) {
        case 'plan':
            return base + 'Help the user plan software implementation. Provide clear, actionable steps. Be concise.' + announce;
        case 'build':
            return base + 'Help the user build and debug code. Provide code snippets and explanations. Be concise.' + announce;
        case 'ask':
            return base + 'Answer questions about the codebase. Be helpful and concise. If the user has open files, reference them when relevant.' + announce;
        default:
            return base + announce;
    }
}

function setupAgentStreamListeners() {
    if (!tauriEvent || !tauriEvent.listen) return;

    tauriEvent.listen('agent-chunk', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        appendToStreamingMessage(payload.chunk || '');
    });

    tauriEvent.listen('agent-reasoning', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var container = document.getElementById('chat-messages');
        if (!container) return;
        var indicator = container.querySelector('.thinking-indicator');
        if (indicator) {
            updateReasoningOnIndicator(indicator, payload.chunk || '');
        }
    });

    tauriEvent.listen('agent-tool-call', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var callId = payload.call_id || '';
        var toolName = payload.tool_name || 'Unknown';
        var filePath = payload.file_path || null;
        
        var existingCall = state.toolCalls.find(function(c) { return c.callId === callId; });
        
        switch(payload.type) {
            case 'start': {
                if (!state.hasStartedToolCall) {
                    finalizeThinkingToMessage();
                }
                if (!existingCall) {
                    state.toolCalls.push({
                        toolName: toolName,
                        filePath: filePath,
                        callId: callId,
                        status: 'running',
                        arguments: payload.arguments || null,
                        details: null,
                        timestamp: Date.now(),
                        startTime: Date.now()
                    });
                    addToolLogEntry(toolName, filePath, callId, 'running', { arguments: payload.arguments });
                }
                break;
            }
            case 'complete': {
                if (existingCall) {
                    existingCall.status = 'done';
                    existingCall.durationMs = payload.duration_ms || 0;
                    existingCall.resultSummary = payload.result_summary || '';
                    existingCall.outputPreview = payload.output_preview || null;
                    updateToolLogEntry(callId, 'done', {
                        resultSummary: payload.result_summary,
                        outputPreview: payload.output_preview,
                        durationMs: payload.duration_ms
                    });
                } else {
                    state.toolCalls.push({
                        toolName: toolName,
                        filePath: filePath,
                        callId: callId,
                        status: 'done',
                        details: null,
                        timestamp: Date.now(),
                        startTime: Date.now()
                    });
                    addToolLogEntry(toolName, filePath, callId, 'done', {
                        resultSummary: payload.result_summary,
                        outputPreview: payload.output_preview,
                        durationMs: payload.duration_ms
                    });
                }
                break;
            }
            case 'error': {
                if (existingCall) {
                    existingCall.status = 'error';
                    existingCall.error = payload.error || '';
                    updateToolLogEntry(callId, 'error', { error: payload.error });
                } else {
                    state.toolCalls.push({
                        toolName: toolName,
                        filePath: filePath,
                        callId: callId,
                        status: 'error',
                        error: payload.error || '',
                        timestamp: Date.now()
                    });
                    addToolLogEntry(toolName, filePath, callId, 'error', { error: payload.error });
                }
                break;
            }
            default: {
                var status = payload.status || 'running';
                var details = payload.details || null;
                if (!existingCall) {
                    state.toolCalls.push({
                        toolName: toolName,
                        filePath: filePath,
                        callId: callId,
                        status: status,
                        details: details,
                        timestamp: Date.now(),
                        startTime: Date.now()
                    });
                    addToolLogEntry(toolName, filePath, callId, status, details);
                } else {
                    existingCall.status = status;
                    existingCall.details = details;
                    updateToolLogEntry(callId, status, details);
                }
            }
        }
    });

    tauriEvent.listen('agent-done', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var container = document.getElementById('chat-messages');
        var indicator = container ? container.querySelector('.thinking-indicator') : null;
        if (indicator) {
            finalizeThinkingToMessage();
        }
        finalizeStreamingMessage();
        if (payload.tokens_used) {
            state.tokenUsage.session += payload.tokens_used;
            state.tokenUsage.total += payload.tokens_used;
            state.tokenBreakdown.total += payload.tokens_used;
            state.lastRequestTokens = payload.tokens_used;
            if (payload.tokens_input) {
                state.tokenBreakdown.input += payload.tokens_input;
            }
            if (payload.tokens_output) {
                state.tokenBreakdown.output += payload.tokens_output;
            } else if (payload.tokens_used) {
                state.tokenBreakdown.output += payload.tokens_used;
            }
            updateTokenDisplay();
            updateTokenUsage(state.tokenBreakdown.total);
        }
        var chat = document.getElementById('chat-messages');
        state.toolCalls.forEach(function(call) {
            if (call.status === 'running') {
                call.status = 'done';
                updateToolLogEntry(call.callId, 'done', call.details || {});
            }
        });
    });

    tauriEvent.listen('agent-error', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        finalizeStreamingMessage();
        addChatMessage('assistant', 'Error: ' + (payload.error || 'unknown'));
    });

    tauriEvent.listen('agent-task-progress', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        state.taskProgress.currentTask = payload.task_number;
        state.taskProgress.totalTasks = payload.total_tasks;
        state.taskProgress.runId = payload.run_id;
        updateTaskProgressUI();
    });

    tauriEvent.listen('agent-task-complete', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var taskNum = payload.task_number;
        var existing = state.taskProgress.tasks.find(function(t) { return t.number === taskNum; });
        if (existing) {
            existing.status = payload.success ? 'completed' : 'failed';
            existing.error = payload.error || null;
        } else {
            state.taskProgress.tasks.push({
                number: taskNum,
                title: '',
                status: payload.success ? 'completed' : 'failed',
                error: payload.error || null,
                diffCount: payload.diff_count || 0,
            });
        }
        updateTaskProgressUI();
    });

    tauriEvent.listen('agent-task-paused', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        state.taskProgress.paused = true;
        state.taskProgress.pausedProposals = payload.proposals || [];
        state.taskProgress.currentTask = payload.task_number;
        showTaskReviewPopup(payload.task_number, payload.task_title, payload.proposals || []);
    });

    tauriEvent.listen('agent-compacted', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var container = document.getElementById('chat-messages');
        if (!container) return;
        var sysMsg = document.createElement('div');
        sysMsg.className = 'chat-message system compacted';
        sysMsg.innerHTML = '<div class="system-icon">📦</div><div class="system-content"><strong>Context compacted</strong><div class="compacted-msg">' + escapeHtml(payload.message || 'Context was compressed to free up space') + '</div></div>';
        container.appendChild(sysMsg);
        container.scrollTop = container.scrollHeight;
    });

    tauriEvent.listen('subagent-start', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var bar = document.getElementById('subagent-status-bar');
        if (bar) {
            bar.classList.remove('hidden');
            bar.innerHTML = '<div class="subagent-chip running"><span class="agent-icon">🤖</span><span class="agent-name">' + escapeHtml(payload.agent) + '</span><span class="agent-status">running…</span></div>';
        }
        addSubagentActivityMessage(payload.agent, payload.task || 'Starting investigation...', 'running');
    });

    tauriEvent.listen('subagent-complete', function(event) {
        var payload = event.payload || {};
        if (state.currentRunId && payload.run_id !== state.currentRunId) return;
        var bar = document.getElementById('subagent-status-bar');
        if (bar) {
            var duration = payload.duration_ms ? ' (' + payload.duration_ms + 'ms)' : '';
            var success = payload.success !== false;
            bar.innerHTML = '<div class="subagent-chip ' + (success ? 'done' : 'error') + '"><span class="agent-icon">' + (success ? '✅' : '❌') + '</span><span class="agent-name">' + escapeHtml(payload.agent) + '</span><span class="agent-status">' + (success ? 'done' : 'failed') + duration + '</span></div>';
            setTimeout(function() { bar.classList.add('hidden'); }, 3000);
        }
        var success = payload.success !== false;
        var duration = payload.duration_ms ? Math.round(payload.duration_ms / 1000) + 's' : '';
        updateSubagentActivityMessage(payload.agent, success ? 'completed' : 'failed', duration, payload.findings);
    });
}

function addSubagentActivityMessage(agentName, task, status) {
    var container = document.getElementById('chat-messages');
    if (!container) return;

    var existing = container.querySelector('.subagent-activity[data-agent="' + escapeHtml(agentName) + '"]');
    if (existing) {
        existing.querySelector('.subagent-task').textContent = task;
        existing.querySelector('.subagent-status-text').textContent = status === 'running' ? 'Investigating…' : status;
        return;
    }

    var card = document.createElement('div');
    card.className = 'subagent-activity';
    card.dataset.agent = agentName;

    var header = document.createElement('div');
    header.className = 'subagent-activity-header';

    var icon = document.createElement('span');
    icon.className = 'subagent-activity-icon';
    icon.textContent = '';

    var name = document.createElement('span');
    name.className = 'subagent-activity-name';
    name.textContent = agentName.charAt(0).toUpperCase() + agentName.slice(1);

    var statusText = document.createElement('span');
    statusText.className = 'subagent-status-text';
    statusText.textContent = status === 'running' ? 'Investigating…' : status;

    header.appendChild(icon);
    header.appendChild(name);
    header.appendChild(statusText);

    var taskEl = document.createElement('div');
    taskEl.className = 'subagent-task';
    taskEl.textContent = task;

    var findingsEl = document.createElement('div');
    findingsEl.className = 'subagent-findings';
    findingsEl.style.display = 'none';

    card.appendChild(header);
    card.appendChild(taskEl);
    card.appendChild(findingsEl);

    container.appendChild(card);
    container.scrollTop = container.scrollHeight;
}

function updateSubagentActivityMessage(agentName, status, duration, findings) {
    var container = document.getElementById('chat-messages');
    if (!container) return;

    var card = container.querySelector('.subagent-activity[data-agent="' + escapeHtml(agentName) + '"]');
    if (!card) return;

    var statusText = card.querySelector('.subagent-status-text');
    if (statusText) {
        statusText.textContent = status === 'completed' ? 'Done' + (duration ? ' (' + duration + ')' : '') : 'Failed';
        statusText.className = 'subagent-status-text ' + (status === 'completed' ? 'done' : 'error');
    }

    if (findings && findings.length > 0) {
        var findingsEl = card.querySelector('.subagent-findings');
        if (findingsEl) {
            findingsEl.style.display = 'block';
            findingsEl.innerHTML = findings.map(function(f) {
                return '<div class="subagent-finding">• ' + escapeHtml(f) + '</div>';
            }).join('');
        }
    }

    card.classList.add(status === 'completed' ? 'done' : 'error');
}

function appendToStreamingMessage(chunk) {
    var container = document.getElementById('chat-messages');
    if (!container) return;
    var indicator = container.querySelector('.thinking-indicator');
    var target;

    if (indicator && !state.hasStartedToolCall) {
        var thinkingContent = indicator.querySelector('.thinking-content');
        if (!thinkingContent) {
            thinkingContent = document.createElement('div');
            thinkingContent.className = 'thinking-content';
            indicator.appendChild(thinkingContent);
        }
        thinkingContent.style.display = 'block';
        thinkingContent.textContent = (thinkingContent.textContent || '') + chunk;
        container.scrollTop = container.scrollHeight;
        state._hasContentBeforeToolCall = true;
        return;
    }

    if (indicator) {
        target = document.createElement('div');
        target.className = 'chat-message assistant streaming';
        target.dataset.rawContent = '';
        var rl = document.createElement('div');
        rl.className = 'chat-role-label';
        rl.textContent = 'Agent';
        target.appendChild(rl);
        var body = document.createElement('div');
        body.className = 'chat-message-body';
        target.appendChild(body);
        container.replaceChild(target, indicator);
    } else {
        var last = container.querySelector('.chat-message.assistant.streaming');
        target = last || null;
        if (!target) {
            target = document.createElement('div');
            target.className = 'chat-message assistant streaming';
            target.dataset.rawContent = '';
            var rl2 = document.createElement('div');
            rl2.className = 'chat-role-label';
            rl2.textContent = 'Agent';
            target.appendChild(rl2);
            var body2 = document.createElement('div');
            body2.className = 'chat-message-body';
            target.appendChild(body2);
            container.appendChild(target);
        }
    }
    target.dataset.rawContent = (target.dataset.rawContent || '') + chunk;
    try {
        target.querySelector('.chat-message-body').innerHTML = renderMarkdown(target.dataset.rawContent);
    } catch (e) {
        target.querySelector('.chat-message-body').textContent = target.dataset.rawContent;
    }
    container.scrollTop = container.scrollHeight;
}

function finalizeStreamingMessage() {
    var container = document.getElementById('chat-messages');
    if (!container) return;
    var streaming = container.querySelector('.chat-message.assistant.streaming');
    if (streaming) {
        var raw = streaming.dataset.rawContent || '';
        var body = streaming.querySelector('.chat-message-body');
        if (body) {
            try {
                body.innerHTML = renderMarkdown(raw);
            } catch (e) {
                body.textContent = raw;
            }
        }
        streaming.classList.remove('streaming');
        delete streaming.dataset.rawContent;
    }
}

function finalizeThinkingToMessage() {
    var container = document.getElementById('chat-messages');
    if (!container) return;
    var indicator = container.querySelector('.thinking-indicator');
    if (!indicator) return;

    var reasoningContent = indicator.dataset.rawContent || '';
    var thinkingEl = indicator.querySelector('.thinking-content');
    var thinkingText = thinkingEl ? thinkingEl.textContent || '' : '';

    var parts = [];
    if (reasoningContent.trim()) parts.push(reasoningContent.trim());
    if (thinkingText.trim()) parts.push(thinkingText.trim());
    var combinedContent = parts.join('\n\n');
    if (!combinedContent.trim()) combinedContent = '...';

    var target = document.createElement('div');
    target.className = 'chat-message assistant streaming';
    target.dataset.rawContent = combinedContent;
    var rl = document.createElement('div');
    rl.className = 'chat-role-label';
    rl.textContent = 'Agent';
    target.appendChild(rl);
    var body = document.createElement('div');
    body.className = 'chat-message-body';
    target.appendChild(body);
    container.replaceChild(target, indicator);

    try {
        body.innerHTML = renderMarkdown(combinedContent);
    } catch (e) {
        body.textContent = combinedContent;
    }
    container.scrollTop = container.scrollHeight;
    state.hasStartedToolCall = true;
}

function updateReasoningOnIndicator(indicator, chunk) {
    if (!indicator) return;
    indicator.dataset.rawContent = (indicator.dataset.rawContent || '') + chunk;

    var dots = indicator.querySelector('.thinking-dots');
    var text = indicator.querySelector('.thinking-text');
    if (dots) dots.style.display = 'none';
    if (text) text.style.display = 'none';

    var header = indicator.querySelector('.reasoning-header');
    var contentEl = indicator.querySelector('.reasoning-content');
    if (!header || !contentEl) return;

    header.style.display = 'flex';
    contentEl.style.display = 'block';
    indicator.classList.add('has-reasoning');

    if (!indicator._reasoningAutoExpanded) {
        indicator.classList.add('expanded');
        indicator._reasoningAutoExpanded = true;
    }

    var fullText = indicator.dataset.rawContent;
    var lines = fullText.split('\n');
    var firstLine = lines[0] || 'Thinking...';
    var restLines = lines.slice(1).join('\n');

    var titleEl = header.querySelector('.reasoning-title');
    if (titleEl) {
        titleEl.textContent = firstLine.substring(0, 80) + (firstLine.length > 80 ? '...' : '');
    }

    contentEl.textContent = restLines;

    var container = document.getElementById('chat-messages');
    if (container) container.scrollTop = container.scrollHeight;
}

function resetReasoningBlock() {
    // No-op: reasoning is now part of the thinking indicator
}



function addThinkingIndicator(mode) {
    var container = document.getElementById('chat-messages');
    if (!container) {
        console.warn('chat-messages container not found');
        return null;
    }

    var emptyState = container.querySelector('.empty-state');
    if (emptyState) emptyState.remove();

    var indicator = document.createElement('div');
    indicator.className = 'thinking-indicator chat-message assistant pending';
    indicator.dataset.mode = mode || 'ask';
    indicator.dataset.rawContent = '';

    var rl = document.createElement('div');
    rl.className = 'chat-role-label';
    rl.textContent = 'Agent';
    indicator.appendChild(rl);

    var dotsRow = document.createElement('div');
    dotsRow.className = 'thinking-dots-row';
    var dots = document.createElement('div');
    dots.className = 'thinking-dots';
    dots.innerHTML = '<span></span><span></span><span></span>';
    dotsRow.appendChild(dots);
    var txt = document.createElement('span');
    txt.className = 'thinking-text';
    txt.textContent = 'Thinking...';
    dotsRow.appendChild(txt);
    indicator.appendChild(dotsRow);

    var reasoningHeader = document.createElement('div');
    reasoningHeader.className = 'reasoning-header';
    reasoningHeader.style.display = 'none';
    reasoningHeader.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M9 18l6-6-6-6"/></svg> <span class="reasoning-title">Thinking...</span>';

    (function(hdr, ind) {
        hdr.addEventListener('click', function(e) {
            e.stopPropagation();
            ind.classList.toggle('expanded');
        });
    })(reasoningHeader, indicator);

    indicator.appendChild(reasoningHeader);

    var reasoningContent = document.createElement('div');
    reasoningContent.className = 'reasoning-content';
    reasoningContent.style.display = 'none';
    indicator.appendChild(reasoningContent);

    var thinkingContent = document.createElement('div');
    thinkingContent.className = 'thinking-content';
    thinkingContent.style.display = 'none';
    indicator.appendChild(thinkingContent);

    container.appendChild(indicator);
    container.scrollTop = container.scrollHeight;
    return indicator;
}

function removeThinkingIndicator(el) {
    if (el && el.parentNode) {
        el.parentNode.removeChild(el);
    }
}

function addChatMessage(role, content, showEdit) {
    var container = document.getElementById('chat-messages');
    var msg = document.createElement('div');
    msg.className = 'chat-message ' + role;
    var label = document.createElement('div');
    label.className = 'chat-role-label';
    label.textContent = role === 'user' ? 'You' : 'Agent';
    msg.appendChild(label);
    var body = document.createElement('div');
    body.className = 'chat-message-body';
    try {
        body.innerHTML = renderMarkdown(content);
    } catch (e) {
        body.textContent = content;
    }
    msg.appendChild(body);

    if (role === 'user' && showEdit !== false) {
        var editBtn = document.createElement('button');
        editBtn.className = 'message-edit-btn';
        editBtn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>';
        editBtn.title = 'Edit message';
        editBtn.addEventListener('click', function() {
            handleEditMessage(msg, content);
        });
        msg.appendChild(editBtn);
    }

    container.appendChild(msg);
    container.scrollTop = container.scrollHeight;
}

function addPlanMessage(role, content) {
    var container = document.getElementById('chat-messages');
    var msg = document.createElement('div');
    msg.className = 'chat-message ' + role;
    var label = document.createElement('div');
    label.className = 'chat-role-label';
    label.textContent = role === 'user' ? 'You' : 'Agent';
    msg.appendChild(label);
    var body = document.createElement('div');
    body.className = 'chat-message-body';
    if (role === 'assistant') {
        body.innerHTML = renderPlanContent(content);
    } else {
        body.innerHTML = renderMarkdown(content);
    }
    msg.appendChild(body);
    container.appendChild(msg);
    container.scrollTop = container.scrollHeight;
}

function addBuildMessage(role, content) {
    var container = document.getElementById('chat-messages');
    var msg = document.createElement('div');
    msg.className = 'chat-message ' + role;
    var label = document.createElement('div');
    label.className = 'chat-role-label';
    label.textContent = role === 'user' ? 'You' : 'Agent';
    msg.appendChild(label);
    var body = document.createElement('div');
    body.className = 'chat-message-body';
    body.innerHTML = renderMarkdown(content);
    msg.appendChild(body);
    container.appendChild(msg);
    container.scrollTop = container.scrollHeight;
}


function toggleSettings() {
    var modal = document.getElementById('settings-modal');
    if (modal) {
        modal.classList.toggle('hidden');
        if (!modal.classList.contains('hidden')) {
            loadSettingsData();
        }
        return;
    }
    
    modal = document.createElement('div');
    modal.id = 'settings-modal';
    modal.className = 'settings-modal';
    modal.innerHTML = '<div class="settings-backdrop"></div>' +
        '<div class="settings-panel">' +
            '<div class="settings-sidebar">' +
                '<div class="sidebar-section">' +
                    '<div class="sidebar-section-title">AI</div>' +
                    '<button class="sidebar-item active" data-tab="providers">' +
                        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>' +
                            '<path d="M7 11V7a5 5 0 0 1 10 0v4"></path>' +
                        '</svg>' +
                        'Providers' +
                    '</button>' +
                    '<button class="sidebar-item" data-tab="models">' +
                        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<path d="M12 2L2 7l10 5 10-5-10-5z"></path>' +
                            '<path d="M2 17l10 5 10-5"></path>' +
                            '<path d="M2 12l10 5 10-5"></path>' +
                        '</svg>' +
                        'Models' +
                    '</button>' +
                '</div>' +
                '<div class="sidebar-section">' +
                    '<div class="sidebar-section-title">Editor</div>' +
                    '<button class="sidebar-item" data-tab="general">' +
                        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<circle cx="12" cy="12" r="3"></circle>' +
                            '<path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path>' +
                        '</svg>' +
                        'General' +
                    '</button>' +
                    '<button class="sidebar-item" data-tab="appearance">' +
                        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<circle cx="13.5" cy="6.5" r="2.5"></circle>' +
                            '<circle cx="17.5" cy="10.5" r="2.5"></circle>' +
                            '<circle cx="8.5" cy="7.5" r="2.5"></circle>' +
                            '<circle cx="6.5" cy="12.5" r="2.5"></circle>' +
                            '<path d="M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12c0 1.74.444 3.37 1.22 4.79"></path>' +
                        '</svg>' +
                        'Appearance' +
                    '</button>' +
                '</div>' +
                '<div class="sidebar-section">' +
                    '<div class="sidebar-section-title">Project</div>' +
                    '<button class="sidebar-item" data-tab="local">' +
                        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect>' +
                            '<line x1="8" y1="21" x2="16" y2="21"></line>' +
                            '<line x1="12" y1="17" x2="12" y2="21"></line>' +
                        '</svg>' +
                        'Local' +
                    '</button>' +
                    '<button class="sidebar-item" data-tab="memory">' +
                        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>' +
                            '<polyline points="14 2 14 8 20 8"></polyline>' +
                            '<line x1="16" y1="13" x2="8" y2="13"></line>' +
                            '<line x1="16" y1="17" x2="8" y2="17"></line>' +
                        '</svg>' +
                        'Memory' +
                    '</button>' +
                '</div>' +
            '</div>' +
            '<div class="settings-main">' +
                '<div class="settings-header">' +
                    '<h3 id="settings-title">Providers</h3>' +
                    '<button id="settings-close" class="btn-icon">' +
                        '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<line x1="18" y1="6" x2="6" y2="18"></line>' +
                            '<line x1="6" y1="6" x2="18" y2="18"></line>' +
                        '</svg>' +
                    '</button>' +
                '</div>' +
                '<div class="settings-body">' +
                    '<div id="settings-tab-providers" class="settings-tab-content active">' +
                        '<div id="keyring-warning-banner" class="keyring-warning-banner hidden">' +
                            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                                '<path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path>' +
                                '<line x1="12" y1="9" x2="12" y2="13"></line>' +
                                '<line x1="12" y1="17" x2="12.01" y2="17"></line>' +
                            '</svg>' +
                            '<span>API keys are stored in plaintext. Install gnome-keyring for encrypted storage.</span>' +
                        '</div>' +
                        '<div id="provider-list"></div>' +
                        '<button id="connect-provider-btn" class="btn-connect-provider">' +
                            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                                '<line x1="12" y1="5" x2="12" y2="19"></line>' +
                                '<line x1="5" y1="12" x2="19" y2="12"></line>' +
                            '</svg>' +
                            'Connect Provider' +
                        '</button>' +
                    '</div>' +
                    '<div id="settings-tab-models" class="settings-tab-content hidden">' +
                        '<div class="models-provider-selector">' +
                            '<label>Select provider</label>' +
                            '<div id="models-provider-dropdown"></div>' +
                        '</div>' +
                        '<div id="models-list"></div>' +
                    '</div>' +
                    '<div id="settings-tab-general" class="settings-tab-content hidden">' +
                        '<div class="settings-section">' +
                            '<div class="section-title">Agent Mode</div>' +
                            '<div class="setting-control">' +
                                '<label>Default mode</label>' +
                                '<div id="settings-default-mode-dropdown"></div>' +
                            '</div>' +
                            '<div class="setting-control">' +
                                '<label>Execution mode</label>' +
                                '<div id="settings-execution-mode-dropdown"></div>' +
                            '</div>' +
                        '</div>' +
                    '</div>' +
                    '<div id="settings-tab-appearance" class="settings-tab-content hidden">' +
                        '<div class="settings-section">' +
                            '<div class="section-title">Appearance</div>' +
                            '<div class="setting-control">' +
                                '<label>Font size</label>' +
                                '<input type="number" id="setting-font-size" value="13" min="10" max="20">' +
                            '</div>' +
                        '</div>' +
                    '</div>' +
                    '<div id="settings-tab-memory" class="settings-tab-content hidden">' +
                        '<div class="settings-section">' +
                            '<div class="section-title">Project Memory</div>' +
                            '<p style="color:var(--text-muted);font-size:var(--font-size-small);margin-bottom:var(--spacing-sm);">Contexto persistente del proyecto. El agente Plan lee este archivo al inicio de cada sesion.</p>' +
                            '<textarea id="setting-memory-content" style="width:100%;height:300px;background:var(--bg-tertiary);border:1px solid var(--border-color);border-radius:var(--border-radius);color:var(--text-primary);padding:var(--spacing-sm);font-family:var(--font-mono);font-size:var(--font-size-small);resize:vertical;outline:none;"></textarea>' +
                        '</div>' +
                    '</div>' +
                    '<div id="settings-tab-local" class="settings-tab-content hidden">' +
                        '<div class="settings-section">' +
                            '<div class="section-title">Ollama Runtime</div>' +
                            '<div class="ollama-status" style="display:flex;align-items:center;gap:var(--spacing-sm);margin-bottom:var(--spacing-sm);">' +
                                '<span id="ollama-status-text">Checking Ollama...</span>' +
                                '<button id="ollama-start-btn" class="btn-small hidden">Start</button>' +
                                '<button id="ollama-stop-btn" class="btn-small btn-reject hidden">Stop</button>' +
                            '</div>' +
                            '<div class="section-title" style="margin-top:var(--spacing-md);">Local Models</div>' +
                            '<div id="ollama-models-list" class="ollama-models-list">' +
                                '<p class="empty-state">No models loaded.</p>' +
                            '</div>' +
                            '<div class="ollama-pull" style="display:flex;gap:var(--spacing-sm);margin-top:var(--spacing-sm);">' +
                                '<input id="ollama-pull-input" type="text" placeholder="Model name (e.g. llama3)" class="ollama-input" style="flex:1;background:var(--bg-tertiary);border:1px solid var(--border-color);border-radius:var(--border-radius);color:var(--text-primary);padding:var(--spacing-sm);">' +
                                '<button id="ollama-pull-btn" class="btn-small">Pull</button>' +
                            '</div>' +
                        '</div>' +
                    '</div>' +
                '</div>' +
                '<div class="settings-footer">' +
                    '<button id="settings-save" class="btn-primary">Save Changes</button>' +
                '</div>' +
            '</div>' +
        '</div>';
    
    document.body.appendChild(modal);
    
    modal.querySelector('.settings-backdrop').addEventListener('click', function() {
        modal.classList.add('hidden');
    });
    
    modal.querySelector('#settings-close').addEventListener('click', function() {
        modal.classList.add('hidden');
    });
    
    modal.querySelectorAll('.sidebar-item').forEach(function(item) {
        item.addEventListener('click', function() {
            switchSettingsTab(item.dataset.tab);
        });
    });
    
    modal.querySelector('#settings-save').addEventListener('click', saveSettings);
    
    var connectBtn = modal.querySelector('#connect-provider-btn');
    if (connectBtn) {
        connectBtn.addEventListener('click', openConnectProviderModal);
    }
    
    loadSettingsData();
}

function switchSettingsTab(tabName) {
    state.activeSettingsTab = tabName;
    
    var modal = document.getElementById('settings-modal');
    if (!modal) return;
    
    modal.querySelectorAll('.sidebar-item').forEach(function(item) {
        item.classList.toggle('active', item.dataset.tab === tabName);
    });
    
    modal.querySelectorAll('.settings-tab-content').forEach(function(content) {
        content.classList.add('hidden');
    });
    var tabContent = modal.querySelector('#settings-tab-' + tabName);
    if (tabContent) tabContent.classList.remove('hidden');
    
    var titles = {
        'providers': 'Providers',
        'models': 'Models',
        'general': 'General',
        'appearance': 'Appearance',
        'memory': 'Memory',
        'local': 'Local (Ollama)'
    };
    var titleEl = modal.querySelector('#settings-title');
    if (titleEl) titleEl.textContent = titles[tabName] || 'Settings';

    if (tabName === 'models') {
        setupModelsProviderDropdown();
    } else if (tabName === 'local') {
        loadOllamaPanel();
    }
    if (tabName === 'memory') {
        loadMemoryEditor();
    }
}

async function loadSettingsData() {
    renderProviderList();
    setupModelsProviderDropdown();
    setupGeneralDropdowns();
    checkKeyringStatus();
}

function loadOllamaPanel() {
    setupOllamaTab();
}

async function checkKeyringStatus() {
    try {
        var status = await invoke('get_keyring_status');
        var banner = document.getElementById('keyring-warning-banner');
        if (banner && status === 'unavailable') {
            banner.classList.remove('hidden');
        }
    } catch (e) {
        console.warn('Failed to check keyring status:', e);
    }
}

async function loadFreeModelsStatus() {
    try {
        var statusContainer = document.getElementById('free-models-status');
        var listContainer = document.getElementById('free-models-list');
        if (!statusContainer || !listContainer) return;

        var models = await invoke('get_free_models_status');
        if (!models || models.length === 0) return;

        statusContainer.classList.remove('hidden');
        listContainer.innerHTML = '';

        models.forEach(function(m) {
            var el = document.createElement('div');
            el.className = 'free-model-item';
            var statusClass = m.available ? 'model-available' : 'model-unavailable';
            var statusText = m.available ? 'Available' : (m.error || 'Unavailable');
            var latencyText = m.latency_ms ? m.latency_ms + 'ms' : '';
            el.innerHTML = '<span class="free-model-name">' + m.model + '</span>' +
                '<span class="free-model-status ' + statusClass + '">' + statusText + ' ' + latencyText + '</span>';
            listContainer.appendChild(el);
        });
    } catch (e) {
        console.warn('Failed to load free models status:', e);
    }
}

function renderProviderList() {
    var container = document.getElementById('provider-list');
    if (!container) return;
    
    container.innerHTML = '';
    
    var keylessProviders = state.providers.filter(function(p) { return p.is_keyless; });
    var connectedProviders = [];
    var notConnectedProviders = [];
    
    state.providers.forEach(function(p) {
        if (p.is_keyless) return;
        if (state.connectedProviders && state.connectedProviders[p.id]) {
            connectedProviders.push(p);
        } else {
            notConnectedProviders.push(p);
        }
    });
    
    if (keylessProviders.length > 0) {
        var section = createProviderSection('Available (no key needed)', keylessProviders, true);
        container.appendChild(section);
    }
    
    if (connectedProviders.length > 0) {
        var section = createProviderSection('Connected', connectedProviders, false);
        container.appendChild(section);
    }
    
    if (notConnectedProviders.length > 0) {
        var section = createProviderSection('Not Connected', notConnectedProviders, false);
        container.appendChild(section);
    }
    
    loadConnectedProviderConfigs();
}

function createProviderSection(title, providers, isKeylessSection) {
    var section = document.createElement('div');
    section.className = 'provider-section';
    
    var header = document.createElement('div');
    header.className = 'provider-section-header';
    header.innerHTML = '<span class="provider-section-line"></span>' +
        '<span class="provider-section-title">' + title + '</span>' +
        '<span class="provider-section-line"></span>';
    section.appendChild(header);
    
    providers.forEach(function(provider) {
        var row = document.createElement('div');
        row.className = 'provider-row';
        row.dataset.providerId = provider.id;
        
        var icon = document.createElement('span');
        icon.className = 'provider-row-icon';
        icon.innerHTML = '<span class="status-dot ' + (isKeylessSection ? 'online' : 'offline') + '"></span>';
        row.appendChild(icon);
        
        var info = document.createElement('div');
        info.className = 'provider-row-info';
        
        var name = document.createElement('span');
        name.className = 'provider-row-name';
        name.textContent = provider.name;
        info.appendChild(name);
        
        var badge = document.createElement('span');
        badge.className = 'provider-row-badge';
        if (provider.is_free) badge.textContent = 'Free';
        else if (provider.is_subscription) badge.textContent = '$10/mo';
        else if (provider.is_payg) badge.textContent = 'PAYG';
        else badge.textContent = 'API Key';
        info.appendChild(badge);
        
        row.appendChild(info);
        
        if (!isKeylessSection) {
            var keyDisplay = document.createElement('span');
            keyDisplay.className = 'provider-row-key';
            keyDisplay.id = 'provider-key-' + provider.id;
            keyDisplay.textContent = '';
            row.appendChild(keyDisplay);
            
            var actions = document.createElement('div');
            actions.className = 'provider-row-actions';
            
            if (state.connectedProviders && state.connectedProviders[provider.id]) {
                var disconnectBtn = document.createElement('button');
                disconnectBtn.className = 'btn-disconnect';
                disconnectBtn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>';
                disconnectBtn.title = 'Disconnect';
                disconnectBtn.addEventListener('click', function(e) {
                    e.stopPropagation();
                    disconnectProvider(provider.id);
                });
                actions.appendChild(disconnectBtn);
            } else {
                var connectBtn = document.createElement('button');
                connectBtn.className = 'btn-connect-small';
                connectBtn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>';
                connectBtn.title = 'Connect';
                connectBtn.addEventListener('click', function(e) {
                    e.stopPropagation();
                    openConnectProviderModal(provider.id);
                });
                actions.appendChild(connectBtn);
            }
            
            row.appendChild(actions);
        }
        
        section.appendChild(row);
    });
    
    return section;
}

async function loadConnectedProviderConfigs() {
    for (var i = 0; i < state.providers.length; i++) {
        var provider = state.providers[i];
        if (provider.is_keyless) continue;
        
        try {
            var config = await invoke('get_provider_config_cmd', { providerId: provider.id });
            if (config.api_key) {
                if (!state.connectedProviders) state.connectedProviders = {};
                state.connectedProviders[provider.id] = true;
                
                var keyEl = document.getElementById('provider-key-' + provider.id);
                if (keyEl) {
                    keyEl.textContent = maskApiKey(config.api_key);
                }
                
                var row = document.querySelector('.provider-row[data-provider-id="' + provider.id + '"]');
                if (row) {
                    var dot = row.querySelector('.status-dot');
                    if (dot) {
                        dot.classList.remove('offline');
                        dot.classList.add('online');
                    }
                }
            }
        } catch (e) {
            
        }
    }
}

function maskApiKey(key) {
    if (!key || key.length < 8) return '••••••••';
    return key.substring(0, 4) + '••••' + key.substring(key.length - 4);
}

function openConnectProviderModal(preselectedProvider) {
    var existing = document.getElementById('connect-provider-modal');
    if (existing) existing.remove();
    
    var providersNeedingKey = state.providers.filter(function(p) { return p.api_key_required && !p.is_keyless; });
    
    var modal = document.createElement('div');
    modal.id = 'connect-provider-modal';
    modal.className = 'connect-provider-modal';
    
    var optionsHtml = '';
    providersNeedingKey.forEach(function(p) {
        var selected = (preselectedProvider === p.id) ? ' selected' : '';
        optionsHtml += '<option value="' + p.id + '"' + selected + '>' + p.name + '</option>';
    });
    
    var defaultProvider = providersNeedingKey.find(function(p) { return p.id === preselectedProvider; }) || providersNeedingKey[0];
    var defaultBaseUrl = defaultProvider ? defaultProvider.base_url : '';
    
    modal.innerHTML = '<div class="connect-provider-backdrop"></div>' +
        '<div class="connect-provider-panel">' +
            '<div class="connect-provider-header">' +
                '<h3>Connect Provider</h3>' +
                '<button class="connect-provider-close">' +
                    '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                        '<line x1="18" y1="6" x2="6" y2="18"></line>' +
                        '<line x1="6" y1="6" x2="18" y2="18"></line>' +
                    '</svg>' +
                '</button>' +
            '</div>' +
            '<div class="connect-provider-body">' +
                '<div class="connect-provider-field">' +
                    '<label>Provider</label>' +
                    '<select id="connect-provider-select" class="connect-provider-select">' +
                        optionsHtml +
                    '</select>' +
                '</div>' +
                '<div class="connect-provider-field">' +
                    '<label>API Key</label>' +
                    '<div class="connect-provider-input-wrapper">' +
                        '<input type="password" id="connect-api-key-input" class="connect-provider-input" placeholder="Paste your API key">' +
                        '<button id="connect-toggle-visibility" class="connect-provider-toggle">' +
                            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                                '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path>' +
                                '<circle cx="12" cy="12" r="3"></circle>' +
                            '</svg>' +
                        '</button>' +
                    '</div>' +
                    '<span class="connect-provider-hint">' +
                        '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                            '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect>' +
                            '<path d="M7 11V7a5 5 0 0 1 10 0v4"></path>' +
                        '</svg>' +
                        'Stored in OS keyring' +
                    '</span>' +
                '</div>' +
                '<div class="connect-provider-field">' +
                    '<label>Base URL <span class="optional">(optional)</span></label>' +
                    '<input type="text" id="connect-base-url-input" class="connect-provider-input" value="' + defaultBaseUrl + '">' +
                '</div>' +
                '<div id="connect-test-result" class="connect-test-result hidden"></div>' +
            '</div>' +
            '<div class="connect-provider-footer">' +
                '<button id="connect-test-btn" class="btn-test-connection">Test</button>' +
                '<button id="connect-submit-btn" class="btn-primary">Connect</button>' +
            '</div>' +
        '</div>';
    
    document.body.appendChild(modal);
    
    modal.querySelector('.connect-provider-backdrop').addEventListener('click', closeConnectProviderModal);
    modal.querySelector('.connect-provider-close').addEventListener('click', closeConnectProviderModal);
    
    var select = modal.querySelector('#connect-provider-select');
    select.addEventListener('change', function() {
        var provider = state.providers.find(function(p) { return p.id === select.value; });
        if (provider) {
            modal.querySelector('#connect-base-url-input').value = provider.base_url;
        }
    });
    
    modal.querySelector('#connect-toggle-visibility').addEventListener('click', function() {
        var input = modal.querySelector('#connect-api-key-input');
        var isPassword = input.type === 'password';
        input.type = isPassword ? 'text' : 'password';
        this.innerHTML = isPassword ?
            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"></path><line x1="1" y1="1" x2="23" y2="23"></line></svg>' :
            '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path><circle cx="12" cy="12" r="3"></circle></svg>';
    });
    
    modal.querySelector('#connect-test-btn').addEventListener('click', function() {
        testProviderConnectionFromModal();
    });
    
    modal.querySelector('#connect-submit-btn').addEventListener('click', function() {
        connectProvider();
    });
}

function closeConnectProviderModal() {
    var modal = document.getElementById('connect-provider-modal');
    if (modal) modal.remove();
}

async function testProviderConnectionFromModal() {
    var modal = document.getElementById('connect-provider-modal');
    if (!modal) return;
    
    var providerId = modal.querySelector('#connect-provider-select').value;
    var apiKey = modal.querySelector('#connect-api-key-input').value.trim();
    var baseUrl = modal.querySelector('#connect-base-url-input').value.trim();
    var resultEl = modal.querySelector('#connect-test-result');
    var testBtn = modal.querySelector('#connect-test-btn');
    
    if (!apiKey) {
        resultEl.textContent = 'Please enter an API key';
        resultEl.className = 'connect-test-result error';
        resultEl.classList.remove('hidden');
        return;
    }
    
    testBtn.disabled = true;
    testBtn.textContent = 'Testing...';
    resultEl.classList.add('hidden');
    
    try {
        var result = await invoke('test_provider_connection', {
            providerId: providerId,
            apiKey: apiKey,
            baseUrl: baseUrl || null
        });
        
        resultEl.textContent = result.message + (result.latency_ms ? ' (' + result.latency_ms + 'ms)' : '');
        resultEl.className = 'connect-test-result ' + (result.success ? 'success' : 'error');
        resultEl.classList.remove('hidden');
    } catch (e) {
        resultEl.textContent = 'Connection failed: ' + e;
        resultEl.className = 'connect-test-result error';
        resultEl.classList.remove('hidden');
    } finally {
        testBtn.disabled = false;
        testBtn.textContent = 'Test';
    }
}

async function connectProvider() {
    var modal = document.getElementById('connect-provider-modal');
    if (!modal) return;
    
    var providerId = modal.querySelector('#connect-provider-select').value;
    var apiKey = modal.querySelector('#connect-api-key-input').value.trim();
    var baseUrl = modal.querySelector('#connect-base-url-input').value.trim();
    var submitBtn = modal.querySelector('#connect-submit-btn');
    
    if (!apiKey) {
        var resultEl = modal.querySelector('#connect-test-result');
        resultEl.textContent = 'Please enter an API key';
        resultEl.className = 'connect-test-result error';
        resultEl.classList.remove('hidden');
        return;
    }
    
    submitBtn.disabled = true;
    submitBtn.textContent = 'Connecting...';
    
    try {
        await invoke('save_provider_config', {
            input: {
                provider_id: providerId,
                api_key: apiKey,
                base_url: baseUrl || null
            }
        });
        
        if (!state.connectedProviders) state.connectedProviders = {};
        state.connectedProviders[providerId] = true;
        
        closeConnectProviderModal();
        renderProviderList();
        
        showToast('Provider connected successfully', 'success');
    } catch (e) {
        var resultEl = modal.querySelector('#connect-test-result');
        resultEl.textContent = 'Failed to save: ' + e;
        resultEl.className = 'connect-test-result error';
        resultEl.classList.remove('hidden');
    } finally {
        submitBtn.disabled = false;
        submitBtn.textContent = 'Connect';
    }
}

async function disconnectProvider(providerId) {
    var provider = state.providers.find(function(p) { return p.id === providerId; });
    if (!provider) return;
    
    if (!confirm('Disconnect ' + provider.name + '? This will remove the stored API key.')) {
        return;
    }
    
    try {
        await invoke('delete_provider_api_key_cmd', { providerId: providerId });
        
        if (state.connectedProviders) {
            delete state.connectedProviders[providerId];
        }
        
        renderProviderList();
        showToast('Provider disconnected', 'success');
    } catch (e) {
        showToast('Failed to disconnect: ' + e, 'error');
    }
}

function showToast(message, type) {
    var toast = document.createElement('div');
    toast.className = 'toast-' + (type || 'success');
    toast.textContent = message;
    document.body.appendChild(toast);
    setTimeout(function() { toast.remove(); }, 3000);
}

async function saveSettings() {
    var modal = document.getElementById('settings-modal');
    if (!modal) return;
    
    modal.classList.add('hidden');
    showToast('Settings saved', 'success');
}

function setupModelsProviderDropdown() {
    var container = document.getElementById('models-provider-dropdown');
    if (!container) return;
    
    container.innerHTML = '';
    var dropdown = createCustomDropdown(
        state.providers.map(function(p) { return { value: p.id, label: p.name }; }),
        state.providers.length > 0 ? state.providers[0].id : '',
        function(value) {
            state.settingsModelProvider = value;
            loadProviderModels(value);
        }
    );
    container.appendChild(dropdown);
    
    loadProviderModels(state.providers.length > 0 ? state.providers[0].id : '');
}

function setupGeneralDropdowns() {
    var defaultModeContainer = document.getElementById('settings-default-mode-dropdown');
    var executionModeContainer = document.getElementById('settings-execution-mode-dropdown');
    
    if (defaultModeContainer) {
        defaultModeContainer.innerHTML = '';
        defaultModeContainer.appendChild(createCustomDropdown(
            [{ value: 'plan', label: 'Plan' }, { value: 'build', label: 'Build' }, { value: 'ask', label: 'Ask' }],
            'plan',
            function() {}
        ));
    }
    
    if (executionModeContainer) {
        executionModeContainer.innerHTML = '';
        executionModeContainer.appendChild(createCustomDropdown(
            [{ value: 'assisted', label: 'Assisted' }, { value: 'autonomous', label: 'Autonomous' }],
            'assisted',
            function() {}
        ));
    }
}

function createCustomDropdown(options, defaultValue, onChange) {
    var wrapper = document.createElement('div');
    wrapper.className = 'custom-dropdown';
    
    var selectedOption = options.find(function(o) { return o.value === defaultValue; }) || options[0];
    
    var trigger = document.createElement('button');
    trigger.className = 'custom-dropdown-trigger';
    trigger.innerHTML = '<span>' + (selectedOption ? selectedOption.label : 'Select...') + '</span>' +
        '<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 15 12 9 18 15"></polyline></svg>';
    
    var menu = document.createElement('div');
    menu.className = 'custom-dropdown-menu hidden';
    
    options.forEach(function(opt) {
        var optionEl = document.createElement('div');
        optionEl.className = 'custom-dropdown-option';
        if (opt.value === defaultValue) optionEl.classList.add('selected');
        optionEl.textContent = opt.label;
        optionEl.addEventListener('click', function() {
            trigger.querySelector('span').textContent = opt.label;
            menu.querySelectorAll('.custom-dropdown-option').forEach(function(o) { o.classList.remove('selected'); });
            optionEl.classList.add('selected');
            menu.classList.add('hidden');
            onChange(opt.value);
        });
        menu.appendChild(optionEl);
    });
    
    trigger.addEventListener('click', function(e) {
        e.stopPropagation();
        menu.classList.toggle('hidden');
    });
    
    document.addEventListener('click', function(e) {
        if (!wrapper.contains(e.target)) {
            menu.classList.add('hidden');
        }
    });
    
    wrapper.appendChild(trigger);
    wrapper.appendChild(menu);
    return wrapper;
}

async function loadProviderModels(providerId) {
    var container = document.getElementById('models-list');
    if (!container) return;
    
    container.innerHTML = '<div class="models-loading">Loading models...</div>';
    
    try {
        var models = await invoke('fetch_provider_models', { providerId: providerId });
        container.innerHTML = '';
        
        if (models.length === 0) {
            var provider = state.providers.find(function(p) { return p.id === providerId; });
            if (provider && provider.default_models.length > 0) {
                models = provider.default_models.map(function(id) {
                    return { id: id, name: id, description: null };
                });
            }
        }
        
        if (models.length === 0) {
            container.innerHTML = '<div class="empty-state">No models available for this provider</div>';
            return;
        }
        
        models.forEach(function(model) {
            var item = document.createElement('div');
            item.className = 'model-item';
            item.innerHTML = '<span class="model-name">' + model.name + '</span>' +
                (model.description ? '<span class="model-description">' + model.description + '</span>' : '');
            container.appendChild(item);
        });
    } catch (err) {
        var provider = state.providers.find(function(p) { return p.id === providerId; });
        if (provider && provider.default_models.length > 0) {
            container.innerHTML = '';
            provider.default_models.forEach(function(modelId) {
                var item = document.createElement('div');
                item.className = 'model-item';
                item.innerHTML = '<span class="model-name">' + modelId + '</span>' +
                    '<span class="model-description">(default)</span>';
                container.appendChild(item);
            });
        } else {
            container.innerHTML = '<div class="empty-state">Failed to load models: ' + err + '</div>';
        }
    }
}

async function saveSettings() {
    var modal = document.getElementById('settings-modal');
    if (!modal) return;
    
    var savePromises = [];
    var results = {};
    
    state.providers.forEach(function(provider) {
        var apiKeyInput = modal.querySelector('.api-key-input[data-provider="' + provider.id + '"]');
        var baseUrlInput = modal.querySelector('.base-url-input[data-provider="' + provider.id + '"]');
        
        var apiKey = apiKeyInput ? apiKeyInput.value.trim() : '';
        var baseUrl = baseUrlInput ? baseUrlInput.value.trim() : '';
        
        savePromises.push(
            invoke('save_provider_config', {
                input: {
                    provider_id: provider.id,
                    api_key: apiKey || null,
                    base_url: baseUrl || null
                }
            }).then(function(result) {
                results[provider.id] = { success: true, message: result };
            }).catch(function(e) {
                results[provider.id] = { success: false, message: e };
            })
        );
    });
    
    await Promise.all(savePromises);
    
    var errors = [];
    var warnings = [];
    state.providers.forEach(function(provider) {
        var r = results[provider.id];
        if (r) {
            if (!r.success) {
                errors.push(provider.name + ': ' + r.message);
            } else if (r.message && r.message.indexOf('plaintext') !== -1) {
                warnings.push(provider.name + ': saved without keyring encryption');
            }
        }
    });
    
    if (errors.length > 0) {
        alert('Errors saving settings:\n' + errors.join('\n'));
    } else if (warnings.length > 0) {
        var toast = document.createElement('div');
        toast.className = 'toast-warning';
        toast.textContent = warnings.join('. ') + '. Install gnome-keyring for encrypted storage.';
        document.body.appendChild(toast);
        setTimeout(function() { toast.remove(); }, 5000);
    } else {
        var toast = document.createElement('div');
        toast.className = 'toast-success';
        toast.textContent = 'All settings saved successfully.';
        document.body.appendChild(toast);
        setTimeout(function() { toast.remove(); }, 3000);
    }
    
    state.providers.forEach(function(provider) {
        var apiKeyInput = modal.querySelector('.api-key-input[data-provider="' + provider.id + '"]');
        var statusDot = document.getElementById('status-' + provider.id);
        if (statusDot) {
            if (apiKeyInput && apiKeyInput.value.trim()) {
                statusDot.classList.add('online');
                statusDot.classList.remove('offline');
            } else if (!provider.api_key_required) {
                statusDot.classList.add('online');
                statusDot.classList.remove('offline');
            } else {
                statusDot.classList.add('offline');
                statusDot.classList.remove('online');
            }
        }
    });
    
    modal.classList.add('hidden');
}


async function updateGitStatus() {
    if (!state.workspacePath) return;

    try {
        var status = await invoke('get_git_status');
        var gitElement = document.getElementById('git-branch');
        var branchName = document.getElementById('branch-name');

        if (status.branch && status.branch !== '-') {
            gitElement.classList.remove('hidden');
            branchName.textContent = status.branch;
        } else {
            gitElement.classList.add('hidden');
        }
    } catch (err) {
        console.log('No es un repositorio git o error:', err);
    }
}


function updateUI() {
    document.getElementById('agents-panel').classList.toggle('collapsed', !state.agentsPanelVisible);
}


function setupStopButton() {
    document.getElementById('agent-stop').addEventListener('click', cancelAgent);
}

async function cancelAgent() {
    try {
        await invoke('cancel_agent');
        state.isLoadingCompletion = false;
        document.getElementById('agent-stop').classList.add('hidden');
        document.getElementById('agent-send').classList.remove('hidden');
    } catch (e) {
        console.error('Error cancelling agent:', e);
    }
}

function showStopButton() {
    document.getElementById('agent-stop').classList.remove('hidden');
    document.getElementById('agent-send').classList.add('hidden');
}

function hideStopButton() {
    document.getElementById('agent-stop').classList.add('hidden');
    document.getElementById('agent-send').classList.remove('hidden');
}

function truncateConversation(history) {
    var MAX_MESSAGES = 40;
    if (history.length <= MAX_MESSAGES) return history;
    var first = history.slice(0, 2);
    var last = history.slice(-(MAX_MESSAGES - 2));
    return first.concat(last);
}

function pushHistorySnapshot() {
    state.historySnapshots.push({
        index: state.conversationHistory.length,
        history: JSON.parse(JSON.stringify(state.conversationHistory)),
        timestamp: Date.now()
    });
    if (state.historySnapshots.length > 50) {
        state.historySnapshots.shift();
    }
}

function rollbackToSnapshot(snapshotIndex) {
    var snapshot = state.historySnapshots[snapshotIndex];
    if (!snapshot) return false;

    state.conversationHistory = JSON.parse(JSON.stringify(snapshot.history));

    state.historySnapshots = state.historySnapshots.slice(0, snapshotIndex + 1);

    var container = document.getElementById('chat-messages');
    if (!container) return true;

    container.innerHTML = '';
    state.conversationHistory.forEach(function(msg) {
        if (msg.role === 'user') {
            addChatMessage('user', msg.content, true);
        } else {
            addChatMessage('assistant', msg.content);
        }
    });

    return true;
}

function getLatestSnapshotIndex() {
    return state.historySnapshots.length - 1;
}

function handleEditMessage(msgElement, originalContent) {
    var snapshotIndex = getLatestSnapshotIndex();
    if (snapshotIndex < 0) return;

    rollbackToSnapshot(snapshotIndex);

    var input = document.getElementById('chat-input');
    if (input) {
        input.value = originalContent;
        input.focus();
        input.style.height = 'auto';
        input.style.height = input.scrollHeight + 'px';
    }
}

function showDiffViewer(filePath, diffContent) {
    document.getElementById('editor-content').classList.add('hidden');
    document.getElementById('editor-placeholder').classList.add('hidden');
    document.getElementById('diff-viewer').classList.remove('hidden');
    document.getElementById('diff-file-path').textContent = filePath;
    renderDiff(diffContent);
}

function hideDiffViewer() {
    document.getElementById('diff-viewer').classList.add('hidden');
    if (state.activeFile) {
        document.getElementById('editor-content').classList.remove('hidden');
    } else {
        document.getElementById('editor-placeholder').classList.remove('hidden');
    }
}

function renderDiff(diffContent) {
    var container = document.getElementById('diff-content');
    container.innerHTML = '';
    var lines = diffContent.split('\n');
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i];
        var div = document.createElement('div');
        div.className = 'diff-line';
        if (line.startsWith('---') || line.startsWith('+++') || line.startsWith('@@')) {
            div.classList.add('header');
        } else if (line.startsWith('+')) {
            div.classList.add('added');
        } else if (line.startsWith('-')) {
            div.classList.add('removed');
        } else {
            div.classList.add('context');
        }
        div.textContent = line || ' ';
        container.appendChild(div);
    }
}

function setupDiffActions() {
}

var currentProposals = [];
var inlineDiffState = { active: false, proposal: null, originalContent: '' };

function showDiffProposals(proposals) {
    currentProposals = proposals;
    var container = document.getElementById('chat-messages');
    if (!container) return;
    if (!proposals || proposals.length === 0) return;

    var card = document.createElement('div');
    card.className = 'diff-summary-card';
    card.innerHTML = '<div class="diff-summary-title">' +
        '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>' +
        ' ' + proposals.length + ' change' + (proposals.length === 1 ? '' : 's') + ' proposed</div>';

    proposals.forEach(function(proposal) {
        var stats = diffStats(proposal.unified_diff || '');
        var row = document.createElement('div');
        row.className = 'diff-summary-row';
        var isActive = state.activeFile === proposal.file_path;
        row.innerHTML =
            '<span class="diff-summary-kind badge-' + String(proposal.kind || 'modify').toLowerCase() + '">' + escapeHtml(String(proposal.kind || 'modify')) + '</span>' +
            '<span class="diff-summary-path">' + escapeHtml(proposal.file_path) + '</span>' +
            '<span class="diff-summary-stats">' +
                '<span class="diff-add-count">+' + stats.added + '</span>' +
                '<span class="diff-del-count">-' + stats.removed + '</span>' +
            '</span>' +
            '<button class="btn-small diff-view-btn" data-id="' + proposal.id + '">' + (isActive ? 'Inline' : 'View') + '</button>';
        row.querySelector('.diff-view-btn').addEventListener('click', function() {
            if (isActive) {
                showInlineDiff(proposal);
            } else {
                openFile(proposal.file_path, proposal.file_path.split('/').pop()).then(function() {
                    setTimeout(function() { showInlineDiff(proposal); }, 100);
                });
            }
        });
        card.appendChild(row);
        
        addPendingDiff(proposal.file_path, proposal.before || '', proposal.after || '', proposal.unified_diff || '');
    });
    container.appendChild(card);
    container.scrollTop = container.scrollHeight;
}

function showInlineDiff(proposal) {
    if (state.activeFile !== proposal.file_path) return;
    clearInlineDiffs();

    var editor = document.getElementById('code-editor');
    var currentContent = getEditorContent();
    inlineDiffState = { active: true, proposal: proposal, originalContent: currentContent };

    clearHighlights();
    hlState.currentLangId = null;

    var isCreate = proposal.kind === 'Create' || proposal.kind === 'create';
    var isDelete = proposal.kind === 'Delete' || proposal.kind === 'delete';
    var oldLines = (isCreate || isDelete) ? [] : currentContent.split('\n');
    var diffLines = parseUnifiedDiff(proposal.unified_diff || '');
    var result = buildDiffLines(oldLines, diffLines, isCreate, isDelete);

    var html = '';
    result.forEach(function(line) {
        var escaped = escapeHtml(line.text);
        if (line.type === 'added') {
            html += '<div class="diff-line-added">' + escaped + '</div>';
        } else if (line.type === 'removed') {
            html += '<div class="diff-line-removed">' + escaped + '</div>';
        } else {
            html += escaped + '\n';
        }
    });

    editor.innerHTML = html.replace(/\n$/, '');
    editor.classList.add('has-inline-diff');
    updateInlineDiffGutter(result);
    updateInlineDiffToolbar(proposal);
}

function parseUnifiedDiff(diffText) {
    var result = { hunks: [] };
    var currentHunk = null;
    diffText.split('\n').forEach(function(line) {
        if (line.startsWith('@@')) {
            var match = line.match(/@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/);
            if (match) {
                currentHunk = {
                    oldStart: parseInt(match[1], 10),
                    oldCount: match[2] ? parseInt(match[2], 10) : 1,
                    newStart: parseInt(match[3], 10),
                    newCount: match[4] ? parseInt(match[4], 10) : 1,
                    lines: []
                };
                result.hunks.push(currentHunk);
            }
        } else if (currentHunk) {
            if (line.startsWith('+')) {
                currentHunk.lines.push({ type: 'added', content: line.substring(1) });
            } else if (line.startsWith('-')) {
                currentHunk.lines.push({ type: 'removed', content: line.substring(1) });
            } else if (!line.startsWith('---') && !line.startsWith('+++')) {
                currentHunk.lines.push({ type: 'context', content: line.substring(1) });
            }
        }
    });
    return result;
}

function buildDiffLines(oldLines, diffParsed, isCreate, isDelete) {
    var result = [];
    var hunks = diffParsed.hunks || [];

    if (hunks.length === 0) {
        return oldLines.map(function(l) { return { type: 'context', text: l }; });
    }

    hunks.forEach(function(hunk) {
        var oldIdx = hunk.oldStart - 1;
        var newIdx = hunk.newStart - 1;
        var oldPos = 0;
        var newPos = 0;

        hunk.lines.forEach(function(dl) {
            switch (dl.type) {
                case 'context':
                    if (!isCreate && oldIdx < oldLines.length) {
                        result.push({ type: 'context', text: oldLines[oldIdx] });
                    }
                    oldIdx++;
                    newIdx++;
                    break;
                case 'removed':
                    if (!isCreate && oldIdx < oldLines.length) {
                        result.push({ type: 'removed', text: oldLines[oldIdx] });
                    }
                    oldIdx++;
                    break;
                case 'added':
                    result.push({ type: 'added', text: dl.content });
                    newIdx++;
                    break;
            }
        });
    });

    if (!isCreate && !isDelete) {
        var maxOldInDiff = 0;
        hunks.forEach(function(h) {
            var end = h.oldStart - 1 + h.oldCount;
            if (end > maxOldInDiff) maxOldInDiff = end;
        });
        for (var i = maxOldInDiff; i < oldLines.length; i++) {
            result.push({ type: 'context', text: oldLines[i] });
        }
    }

    return result;
}

function updateInlineDiffGutter(result) {
    var lineNumbers = document.getElementById('line-numbers');
    var html = '';
    var lineNum = 1;
    result.forEach(function(line) {
        if (line.type === 'added') {
            html += '<div class="line-number diff-gutter-added">+</div>';
        } else if (line.type === 'removed') {
            html += '<div class="line-number diff-gutter-removed">-</div>';
            lineNum++;
        } else {
            html += '<div class="line-number">' + lineNum + '</div>';
            lineNum++;
        }
    });
    lineNumbers.innerHTML = html;
}

function updateInlineDiffToolbar(proposal) {
    var existing = document.getElementById('inline-diff-toolbar');
    if (existing) existing.remove();

    var editor = document.getElementById('code-editor');
    var toolbar = document.createElement('div');
    toolbar.id = 'inline-diff-toolbar';
    toolbar.className = 'inline-diff-toolbar';
    toolbar.innerHTML =
        '<span class="inline-diff-info">' + escapeHtml(proposal.file_path) + '</span>' +
        '<button class="btn-small btn-accept" id="inline-diff-accept">Accept</button>' +
        '<button class="btn-small btn-reject" id="inline-diff-reject">Reject</button>';
    toolbar.querySelector('#inline-diff-accept').addEventListener('click', function() {
        acceptInlineDiff(proposal);
    });
    toolbar.querySelector('#inline-diff-reject').addEventListener('click', function() {
        rejectInlineDiff(proposal);
    });
    editor.parentElement.insertBefore(toolbar, editor.nextSibling);
}

function acceptInlineDiff(proposal) {
    if (!inlineDiffState.active) return;
    var diff = state.pendingDiffs && state.pendingDiffs[proposal.file_path];
    if (diff) {
        state.fileContents.set(proposal.file_path, diff.newContent);
        state.modifiedFiles.add(proposal.file_path);
    } else {
        state.fileContents.set(proposal.file_path, proposal.after || getEditorContent());
        state.modifiedFiles.add(proposal.file_path);
    }
    setEditorContent(state.fileContents.get(proposal.file_path));
    clearInlineDiffs();
    onEditorInput();
    renderTabs();
}

function rejectInlineDiff(proposal) {
    if (!inlineDiffState.active) return;
    setEditorContent(inlineDiffState.originalContent);
    clearInlineDiffs();
    onEditorInput();
}

function clearInlineDiffs() {
    var editor = document.getElementById('code-editor');
    if (editor) {
        editor.classList.remove('has-inline-diff');
        if (inlineDiffState.active && inlineDiffState.originalContent) {
            setEditorContent(inlineDiffState.originalContent);
        }
    }
    var toolbar = document.getElementById('inline-diff-toolbar');
    if (toolbar) toolbar.remove();
    inlineDiffState = { active: false, proposal: null, originalContent: '' };
    updateLineNumbers();
    scheduleHighlight();
}

function diffStats(diffContent) {
    var added = 0, removed = 0;
    diffContent.split('\n').forEach(function(line) {
        if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('@@')) return;
        if (line.startsWith('+')) added++;
        else if (line.startsWith('-')) removed++;
    });
    return { added: added, removed: removed };
}

function setEditorMode(mode, content) {
    var codeEl = document.getElementById('editor-content');
    var planEl = document.getElementById('plan-mode');
    var diffEl = document.getElementById('diff-mode');
    var placeholderEl = document.getElementById('editor-placeholder');
    if (!codeEl) return;

    [codeEl, planEl, diffEl, placeholderEl].forEach(function(el) {
        if (el) el.classList.add('hidden');
    });

    if (mode === 'code') {
        codeEl.classList.remove('hidden');
    } else if (mode === 'plan') {
        planEl.classList.remove('hidden');
        renderPlanInEditor(content);
    } else if (mode === 'diff') {
        diffEl.classList.remove('hidden');
        renderDiffInEditor(content);
    } else {
        if (placeholderEl) placeholderEl.classList.remove('hidden');
    }
}

function renderPlanInEditor(plan) {
    var bodyEl = document.getElementById('plan-mode-body');
    if (!bodyEl) return;
    var markdown = typeof plan === 'string' ? plan : (plan && plan.content) || '';
    var frontmatter = null;
    var fmMatch = markdown.match(/^---\n([\s\S]*?)\n---\n\n?([\s\S]*)$/);
    if (fmMatch) {
        frontmatter = fmMatch[1];
        markdown = fmMatch[2];
    }
    var planPath = (plan && plan.path) || 'current';
    var planName = (plan && plan.name) || planPath;
    var bodyHtml = (typeof marked !== 'undefined') ? marked.parse(markdown) : '<pre>' + escapeHtml(markdown) + '</pre>';

    bodyEl.innerHTML =
        '<div class="plan-editor-header">' +
            '<div class="plan-editor-title">' + escapeHtml(planName) + '</div>' +
            '<div class="plan-editor-actions">' +
                '<button class="btn-small" data-plan-action="add-comment">Add comment</button>' +
            '</div>' +
        '</div>' +
        '<div class="plan-editor-body" data-plan-path="' + escapeHtml(planPath) + '">' + bodyHtml + '</div>';

    bindPlanCommentButtons(bodyEl, planPath);
}

function bindPlanCommentButtons(container, planPath) {
    var addBtn = container.querySelector('[data-plan-action="add-comment"]');
    if (addBtn) {
        addBtn.addEventListener('click', function() {
            var body = prompt('Comment:');
            if (!body) return;
            invoke('add_plan_comment', {
                input: {
                    target_path: planPath,
                    line: 0,
                    body: body,
                    author: 'user'
                }
            }).then(function() {
                renderPlanCommentList(planPath);
            }).catch(function(e) { console.error(e); });
        });
    }
    renderPlanCommentList(planPath);
}

function renderPlanCommentList(planPath) {
    invoke('list_plan_comments', { planPath: planPath })
        .then(function(comments) {
            var panel = document.getElementById('plan-mode-comments');
            if (!panel) return;
            if (!comments || comments.length === 0) { panel.innerHTML = ''; return; }
            panel.innerHTML = comments.map(function(c) {
                return '<div class="plan-comment' + (c.resolved ? ' resolved' : '') + '">' +
                    '<div class="plan-comment-meta">Line ' + c.line + ' · ' + escapeHtml(c.author) + '</div>' +
                    '<div class="plan-comment-body">' + escapeHtml(c.body) + '</div>' +
                '</div>';
            }).join('');
        })
        .catch(function(e) { console.error(e); });
}

function renderDiffInEditor(proposal) {
    var pathEl = document.getElementById('diff-mode-path');
    var bodyEl = document.getElementById('diff-mode-body');
    if (!bodyEl) return;
    if (pathEl) pathEl.textContent = proposal.file_path;
    var lines = (proposal.unified_diff || '').split('\n');
    bodyEl.innerHTML = '';
    lines.forEach(function(line, idx) {
        var div = document.createElement('div');
        div.className = 'diff-line';
        div.dataset.line = String(idx + 1);
        if (line.startsWith('---') || line.startsWith('+++') || line.startsWith('@@')) {
            div.classList.add('header');
        } else if (line.startsWith('+')) {
            div.classList.add('added');
        } else if (line.startsWith('-')) {
            div.classList.add('removed');
        } else {
            div.classList.add('context');
        }
        div.textContent = line || ' ';
        bodyEl.appendChild(div);
    });
    var actionsEl = document.getElementById('diff-mode-actions');
    if (actionsEl) {
        actionsEl.innerHTML =
            '<button class="btn-small btn-accept" data-id="' + proposal.id + '">Accept</button>' +
            '<button class="btn-small btn-reject" data-id="' + proposal.id + '">Reject</button>' +
            '<button class="btn-small" data-id="' + proposal.id + '" data-action="add-comment">Add comment</button>';
        actionsEl.querySelector('.btn-accept').addEventListener('click', function() { acceptDiff(proposal.id); });
        actionsEl.querySelector('.btn-reject').addEventListener('click', function() { rejectDiff(proposal.id); });
        actionsEl.querySelector('[data-action="add-comment"]').addEventListener('click', function() {
            var body = prompt('Comment:');
            if (!body) return;
            invoke('add_diff_comment', {
                proposalId: proposal.id,
                filePath: proposal.file_path,
                line: 0,
                body: body,
                author: 'user'
            }).then(function() { console.log('comment added'); });
        });
    }
}

async function acceptDiff(id) {
    try {
        await invoke('accept_diff', { proposalId: id });
        currentProposals = currentProposals.filter(function(p) { return p.id !== id; });
        setEditorMode('code');
    } catch (e) { console.error('accept failed:', e); }
}

async function rejectDiff(id) {
    try {
        await invoke('reject_diff', { proposalId: id });
        currentProposals = currentProposals.filter(function(p) { return p.id !== id; });
        setEditorMode('code');
    } catch (e) { console.error('reject failed:', e); }
}


async function loadPlansList() {
    var container = document.getElementById('plans-list');
    try {
        var plans = await invoke('list_plans');
        if (plans.length === 0) {
            container.innerHTML = '<p class="empty-state">No hay planes guardados.</p>';
            return;
        }
        container.innerHTML = '';
        plans.forEach(function(planName) {
            var item = document.createElement('div');
            item.className = 'plan-item';
            item.innerHTML = '<span class="plan-item-name">' + planName + '</span>';
            item.addEventListener('click', function() { loadPlan(planName); });
            container.appendChild(item);
        });
    } catch (e) {
        container.innerHTML = '<p class="empty-state">Error cargando planes: ' + e + '</p>';
    }
}

async function loadPlan(planName) {
    try {
        var content = await invoke('load_plan', { path: planName });
        document.getElementById('plans-list').classList.add('hidden');
        document.getElementById('plan-viewer').classList.remove('hidden');
        document.getElementById('plan-viewer-title').textContent = planName;
        var viewerContent = document.getElementById('plan-viewer-content');
        viewerContent.innerHTML = renderPlanContent(content);
    } catch (e) {
        console.error('Error loading plan:', e);
    }
}

function renderPlanContent(rawContent) {
    var content = rawContent;
    var frontmatter = null;
    var fmMatch = content.match(/^---\n([\s\S]*?)\n---\n\n?([\s\S]*)$/);
    if (fmMatch) {
        frontmatter = fmMatch[1];
        content = fmMatch[2];
    }

    var sections = parsePlanSections(content);
    var html = '';

    if (frontmatter) {
        var fm = parseFrontmatter(frontmatter);
        html += '<div class="plan-section plan-meta">' +
            '<div class="plan-meta-info">' +
                '<span>Provider: ' + escapeHtml(fm.provider || '--') + '</span>' +
                '<span>Generated: ' + escapeHtml(fm.generated_at || '--') + '</span>' +
                '<span>Est. build tokens: ' + escapeHtml(fm.estimated_build_tokens || '--') + '</span>' +
            '</div>' +
        '</div>';
    }

    if (sections.investigations.length > 0) {
        html += '<details class="plan-investigations">' +
            '<summary>Investigations (' + sections.investigations.length + ' web searches)</summary>' +
            '<ul>' + sections.investigations.map(function(q) { return '<li>' + escapeHtml(q) + '</li>'; }).join('') + '</ul>' +
        '</details>';
    }

    if (sections.stack) {
        html += '<div class="plan-section">' +
            '<h2>Stack</h2>' +
            (typeof marked !== 'undefined' ? marked.parse(sections.stack) : '<pre>' + escapeHtml(sections.stack) + '</pre>') +
        '</div>';
    }

    if (sections.alternatives) {
        html += '<div class="plan-section">' +
            '<h2>Alternatives</h2>' +
            renderAlternativesTable(sections.alternatives) +
        '</div>';
    }

    if (sections.steps.length > 0 && sections.tasks.length === 0) {
        html += '<div class="plan-section">' +
            '<h2>Steps</h2>' +
            '<ol class="plan-steps">' +
            sections.steps.map(function(step, i) {
                return '<li class="plan-step">' +
                    '<input type="checkbox" class="plan-step-check" data-step="' + (i + 1) + '">' +
                    '<div class="plan-step-content">' +
                        '<span class="plan-step-number">' + (i + 1) + '.</span> ' +
                        (typeof marked !== 'undefined' ? marked.parseInline(step) : escapeHtml(step)) +
                    '</div>' +
                '</li>';
            }).join('') +
            '</ol>' +
        '</div>';
    }

    if (sections.tasks.length > 0) {
        html += '<div class="plan-section plan-tasks-section">' +
            '<h2>Tasks (' + sections.tasks.length + ')</h2>' +
            '<div class="plan-tasks">' +
            sections.tasks.map(function(task) {
                var detailsHtml = '';
                if (task.goal) detailsHtml += '<div class="plan-task-detail"><strong>Goal:</strong> ' + escapeHtml(task.goal) + '</div>';
                if (task.files) detailsHtml += '<div class="plan-task-detail"><strong>Files:</strong> ' + escapeHtml(task.files) + '</div>';
                if (task.tools) detailsHtml += '<div class="plan-task-detail"><strong>Tools:</strong> ' + escapeHtml(task.tools) + '</div>';
                if (task.depends) detailsHtml += '<div class="plan-task-detail"><strong>Depends:</strong> ' + escapeHtml(task.depends) + '</div>';
                if (task.acceptance) detailsHtml += '<div class="plan-task-detail"><strong>Acceptance:</strong> ' + escapeHtml(task.acceptance) + '</div>';

                return '<details class="plan-task" data-task="' + task.number + '">' +
                    '<summary class="plan-task-header">' +
                        '<input type="checkbox" class="plan-task-check" data-task="' + task.number + '" onclick="event.stopPropagation()">' +
                        '<span class="plan-task-title">Task ' + task.number + ': ' + escapeHtml(task.title) + '</span>' +
                        '<button class="plan-task-implement-btn" data-task-num="' + task.number + '">Implement</button>' +
                    '</summary>' +
                    '<div class="plan-task-body">' + detailsHtml + '</div>' +
                '</details>';
            }).join('') +
            '</div>' +
        '</div>';
    }

    if (sections.risks.length > 0) {
        html += '<div class="plan-section" style="border-left-color: var(--warning);">' +
            '<h2 style="color: var(--warning);">Risks</h2>' +
            '<ul class="plan-risks">' +
            sections.risks.map(function(risk) {
                return '<li class="plan-risk">' +
                    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">' +
                        '<path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path>' +
                        '<line x1="12" y1="9" x2="12" y2="13"></line>' +
                        '<line x1="12" y1="17" x2="12.01" y2="17"></line>' +
                    '</svg>' +
                    escapeHtml(risk) +
                '</li>';
            }).join('') +
            '</ul>' +
        '</div>';
    }

    if (sections.estimation) {
        html += '<div class="plan-section">' +
            '<h2>Estimation</h2>' +
            (typeof marked !== 'undefined' ? marked.parse(sections.estimation) : '<pre>' + escapeHtml(sections.estimation) + '</pre>') +
        '</div>';
    }

    if (sections.other) {
        html += '<div class="plan-section">' +
            (typeof marked !== 'undefined' ? marked.parse(sections.other) : '<pre>' + escapeHtml(sections.other) + '</pre>') +
        '</div>';
    }

    return html;
}

function parsePlanSections(content) {
    var sections = {
        stack: null,
        alternatives: null,
        steps: [],
        tasks: [],
        risks: [],
        estimation: null,
        investigations: [],
        other: null,
    };

    var lines = content.split('\n');
    var currentSection = null;
    var currentContent = [];
    var currentTask = null;

    function flushSection() {
        if (currentTask) {
            sections.tasks.push(currentTask);
            currentTask = null;
        }
        if (!currentSection) return;
        var text = currentContent.join('\n').trim();
        if (!text) return;

        var key = currentSection.toLowerCase();
        if (key.indexOf('stack') !== -1) {
            sections.stack = text;
        } else if (key.indexOf('alternativa') !== -1 || key.indexOf('alternative') !== -1) {
            sections.alternatives = text;
        } else if (key.indexOf('paso') !== -1 || key.indexOf('step') !== -1) {
            sections.steps = parseStepsList(text);
        } else if (key.indexOf('riesgo') !== -1 || key.indexOf('risk') !== -1) {
            sections.risks = parseBulletsList(text);
        } else if (key.indexOf('estimacion') !== -1 || key.indexOf('estimation') !== -1) {
            sections.estimation = text;
        } else if (key.indexOf('investigacion') !== -1 || key.indexOf('investigation') !== -1) {
            sections.investigations = parseBulletsList(text);
        } else {
            if (sections.other) {
                sections.other += '\n\n## ' + currentSection + '\n\n' + text;
            } else {
                sections.other = '## ' + currentSection + '\n\n' + text;
            }
        }
    }

    for (var i = 0; i < lines.length; i++) {
        var line = lines[i];
        var h2Match = line.match(/^## (.+)$/);
        var h3TaskMatch = line.match(/^### Task (\d+):\s*(.*)/);

        if (h2Match) {
            flushSection();
            currentSection = h2Match[1].trim();
            currentContent = [];
        } else if (h3TaskMatch && currentSection && (currentSection.toLowerCase().indexOf('paso') !== -1 || currentSection.toLowerCase().indexOf('step') !== -1)) {
            if (currentTask) {
                sections.tasks.push(currentTask);
            }
            currentTask = {
                number: parseInt(h3TaskMatch[1]),
                title: h3TaskMatch[2].trim(),
                goal: '',
                files: '',
                tools: '',
                depends: '',
                acceptance: '',
                raw: '',
            };
            currentContent = [];
        } else if (currentTask) {
            var goalMatch = line.match(/^\s*[-*]\s*\*\*Goal:\*\*\s*(.*)/);
            var filesMatch = line.match(/^\s*[-*]\s*\*\*Archivos:\*\*\s*(.*)/);
            var toolsMatch = line.match(/^\s*[-*]\s*\*\*Herramientas:\*\*\s*(.*)/);
            var dependsMatch = line.match(/^\s*[-*]\s*\*\*Depende de:\*\*\s*(.*)/);
            var acceptanceMatch = line.match(/^\s*[-*]\s*\*\*Criterio de aceptacion:\*\*\s*(.*)/);

            if (goalMatch) currentTask.goal = goalMatch[1].trim();
            else if (filesMatch) currentTask.files = filesMatch[1].trim();
            else if (toolsMatch) currentTask.tools = toolsMatch[1].trim();
            else if (dependsMatch) currentTask.depends = dependsMatch[1].trim();
            else if (acceptanceMatch) currentTask.acceptance = acceptanceMatch[1].trim();

            currentTask.raw += line + '\n';
        } else {
            currentContent.push(line);
        }
    }
    flushSection();

    return sections;
}

function parseStepsList(text) {
    var steps = [];
    var lines = text.split('\n');
    var currentStep = '';

    for (var i = 0; i < lines.length; i++) {
        var line = lines[i];
        var numMatch = line.match(/^\d+[\.\)]\s+(.*)/);
        if (numMatch) {
            if (currentStep) steps.push(currentStep.trim());
            currentStep = numMatch[1];
        } else if (currentStep && line.trim()) {
            currentStep += '\n' + line;
        }
    }
    if (currentStep) steps.push(currentStep.trim());

    return steps;
}

function parseBulletsList(text) {
    var items = [];
    var lines = text.split('\n');
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].replace(/^[-*]\s*/, '').trim();
        if (line) items.push(line);
    }
    return items;
}

function renderAlternativesTable(text) {
    var lines = text.split('\n');
    var tableLines = lines.filter(function(l) { return l.indexOf('|') !== -1; });
    if (tableLines.length >= 3) {
        var html = '<table>';
        for (var i = 0; i < tableLines.length; i++) {
            var cells = tableLines[i].split('|').filter(function(c) { return c.trim(); }).map(function(c) { return c.trim(); });
            if (i === 0) {
                html += '<thead><tr>' + cells.map(function(c) { return '<th>' + escapeHtml(c) + '</th>'; }).join('') + '</tr></thead><tbody>';
            } else if (i === 1) {
                continue;
            } else {
                html += '<tr>' + cells.map(function(c) { return '<td>' + escapeHtml(c) + '</td>'; }).join('') + '</tr>';
            }
        }
        html += '</tbody></table>';
        return html;
    }
    return typeof marked !== 'undefined' ? marked.parse(text) : '<pre>' + escapeHtml(text) + '</pre>';
}

function parseFrontmatter(text) {
    var result = {};
    var lines = text.split('\n');
    for (var i = 0; i < lines.length; i++) {
        var parts = lines[i].split(':');
        if (parts.length >= 2) {
            var key = parts[0].trim();
            var val = parts.slice(1).join(':').trim();
            result[key] = val;
        }
    }
    return result;
}

function escapeHtml(text) {
    var div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function setupPlanViewer() {
    document.getElementById('plan-back').addEventListener('click', function() {
        document.getElementById('plan-viewer').classList.add('hidden');
        document.getElementById('plans-list').classList.remove('hidden');
    });
}


if (typeof marked !== 'undefined') {
    marked.use({
        breaks: true,
        gfm: true,
    });
}

function renderMarkdown(text) {
    if (typeof marked !== 'undefined') {
        try {
            return marked.parse(text);
        } catch (e) {
            console.warn('marked.parse failed:', e);
        }
    }
    return '<pre>' + escapeHtml(text) + '</pre>';
}

function renderPlanCard(planData) {
    var container = document.getElementById('chat-messages');
    var card = document.createElement('div');
    card.className = 'plan-card';

    var header = document.createElement('div');
    header.className = 'plan-card-header';

    var icon = document.createElement('div');
    icon.className = 'plan-card-icon';
    icon.innerHTML = '<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line></svg>';

    var titleEl = document.createElement('div');
    titleEl.className = 'plan-card-title';
    titleEl.textContent = planData.title || 'Untitled Plan';

    var summaryEl = document.createElement('div');
    summaryEl.className = 'plan-card-summary';
    summaryEl.textContent = planData.summary || '';

    header.appendChild(icon);
    header.appendChild(titleEl);

    var actions = document.createElement('div');
    actions.className = 'plan-card-actions';

    var openBtn = document.createElement('button');
    openBtn.className = 'plan-card-btn plan-card-btn-open';
    openBtn.textContent = 'Open in Editor';
    openBtn.addEventListener('click', function() {
        openPlanInView(planData.file_path, planData.title, planData.content);
    });

    var implementBtn = document.createElement('button');
    implementBtn.className = 'plan-card-btn plan-card-btn-implement';
    implementBtn.textContent = 'Implement All';
    implementBtn.addEventListener('click', function() {
        implementPlan(planData.file_path);
    });

    actions.appendChild(openBtn);
    actions.appendChild(implementBtn);

    card.appendChild(header);
    card.appendChild(summaryEl);
    card.appendChild(actions);

    container.appendChild(card);
    container.scrollTop = container.scrollHeight;
}

function openPlanInView(filePath, title, content) {
    var planView = document.getElementById('plan-editor-view');
    var planBody = document.getElementById('plan-editor-body');
    var planTitle = document.getElementById('plan-editor-title');

    planTitle.textContent = title || 'Plan';
    planBody.innerHTML = renderMarkdown(content || '');

    document.getElementById('editor-content').classList.add('hidden');
    document.getElementById('editor-placeholder').classList.add('hidden');
    planView.classList.remove('hidden');

    planView._filePath = filePath;
    planView._content = content;

    planBody.querySelectorAll('.plan-task-implement-btn').forEach(function(btn) {
        btn.addEventListener('click', function(e) {
            e.stopPropagation();
            var taskNum = parseInt(btn.getAttribute('data-task-num'));
            if (taskNum) {
                implementPlanTask(filePath, taskNum);
            }
        });
    });
}

function closePlanView() {
    var planView = document.getElementById('plan-editor-view');
    planView.classList.add('hidden');

    if (state.activeFile) {
        document.getElementById('editor-content').classList.remove('hidden');
    } else {
        document.getElementById('editor-placeholder').classList.remove('hidden');
    }
}

async function implementPlan(planPath, taskNumbers) {
    if (state.isLoadingCompletion) return;

    setAgentMode('build');
    state.isLoadingCompletion = true;
    var runId = 'build_' + Date.now();
    state.currentRunId = runId;
    state.taskProgress.runId = runId;
    state.taskProgress.tasks = [];
    state.taskProgress.paused = false;
    resetReasoningBlock();
    showStopButton();

    try {
        var result = await invoke('implement_plan_with_agent', {
            planPath: planPath,
            taskNumbers: taskNumbers || null,
            providerId: state.selectedProviderId,
            modelId: state.selectedModelId,
            reasoningEffort: state.reasoningEffort
        });

        if (result && result.proposals && result.proposals.length > 0) {
            showDiffProposals(result.proposals);
            var msg = 'Plan implemented. ' + result.proposals.length + ' change(s) proposed.';
            if (result.tool_log && result.tool_log.length > 0) {
                msg += ' ' + result.tool_log[0];
            }
            addChatMessage('assistant', msg);
        } else if (result && result.tool_log && result.tool_log.length > 0) {
            addChatMessage('assistant', result.tool_log[0]);
        } else {
            addChatMessage('assistant', 'Plan implemented successfully.');
        }
    } catch (err) {
        addChatMessage('assistant', 'Error implementing plan: ' + err);
    } finally {
        hideStopButton();
        state.isLoadingCompletion = false;
        state.currentRunId = null;
    }
}

function implementPlanTask(planPath, taskNumber) {
    implementPlan(planPath, [taskNumber]);
}

function updateTokenUsage(tokens) {
    var el = document.getElementById('token-usage');
    el.textContent = 'Tokens: ' + (tokens || 0);
    el.classList.remove('hidden');
}

function updateActiveModelDisplay() {
    var el = document.getElementById('active-model');
    if (state.selectedProviderId && state.selectedModelId) {
        var provider = state.providers.find(function(p) { return p.id === state.selectedProviderId; });
        el.textContent = (provider ? provider.name : state.selectedProviderId) + ' / ' + state.selectedModelId;
    } else {
        el.textContent = '--';
    }
}





async function loadMemoryEditor() {
    try {
        var content = await invoke('read_memory');
        var textarea = document.getElementById('setting-memory-content');
        if (textarea) {
            textarea.value = content || '';
        }
    } catch (e) {}
}

async function saveMemoryEditor() {
    var textarea = document.getElementById('setting-memory-content');
    if (!textarea) return;
    try {
        await invoke('write_memory', { content: textarea.value });
    } catch (e) {
        console.error('Error saving memory:', e);
    }
}


var originalInit = init;
init = function() {
    originalInit();
    setupStopButton();
    setupDiffActions();
    setupPlanViewer();
    updateActiveModelDisplay();
    setupOllamaTab();
    updateTokenDisplay();
};

function setupOllamaTab() {
    const statusText = document.getElementById('ollama-status-text');
    const startBtn = document.getElementById('ollama-start-btn');
    const stopBtn = document.getElementById('ollama-stop-btn');
    const modelsList = document.getElementById('ollama-models-list');
    const pullInput = document.getElementById('ollama-pull-input');
    const pullBtn = document.getElementById('ollama-pull-btn');

    async function checkStatus() {
        statusText.textContent = 'Checking Ollama...';
        statusText.className = 'ollama-status-checking';
        startBtn.classList.add('hidden');
        stopBtn.classList.add('hidden');

        try {
            const status = await invoke('ollama_status');
            if (status === 'running') {
                statusText.textContent = 'Ollama is running';
                statusText.className = 'ollama-status-running';
                stopBtn.classList.remove('hidden');
                loadModels();
            } else {
                statusText.textContent = 'Ollama is not running';
                statusText.className = 'ollama-status-stopped';
                startBtn.classList.remove('hidden');
                modelsList.innerHTML = '<p class="empty-state">Ollama is not available.</p>';
            }
        } catch (e) {
            statusText.textContent = 'Ollama check failed';
            statusText.className = 'ollama-status-stopped';
            startBtn.classList.remove('hidden');
            modelsList.innerHTML = '<p class="empty-state">Cannot connect to Ollama.</p>';
        }
    }

    async function loadModels() {
        try {
            const models = await invoke('ollama_list_models');
            if (models.length === 0) {
                modelsList.innerHTML = '<p class="empty-state">No local models found.</p>';
                return;
            }
            modelsList.innerHTML = '';
            models.forEach(m => {
                const el = document.createElement('div');
                el.className = 'ollama-model-item';
                el.innerHTML = `<span class="ollama-model-name">${m.name}</span><span class="ollama-model-size">${m.size}</span>`;
                modelsList.appendChild(el);
            });
        } catch (e) {
            modelsList.innerHTML = '<p class="empty-state">Failed to load models.</p>';
        }
    }

    startBtn.addEventListener('click', async () => {
        startBtn.disabled = true;
        startBtn.textContent = 'Starting...';
        try {
            await invoke('ollama_start_runtime');
            setTimeout(checkStatus, 2000);
        } catch (e) {
            alert('Failed to start Ollama: ' + e);
            startBtn.disabled = false;
            startBtn.textContent = 'Start';
        }
    });

    stopBtn.addEventListener('click', async () => {
        stopBtn.disabled = true;
        try {
            await invoke('ollama_stop_runtime');
            setTimeout(checkStatus, 1000);
        } catch (e) {
            alert('Failed to stop Ollama: ' + e);
            stopBtn.disabled = false;
        }
    });

    pullBtn.addEventListener('click', async () => {
        const model = pullInput.value.trim();
        if (!model) return;

        pullBtn.disabled = true;
        pullBtn.textContent = 'Pulling...';

        try {
            await invoke('ollama_pull_model', { model });
            pullInput.value = '';
            loadModels();
        } catch (e) {
            alert('Failed to pull model: ' + e);
        } finally {
            pullBtn.disabled = false;
            pullBtn.textContent = 'Pull';
        }
    });

    checkStatus();
}

function updateTaskProgressUI() {
    var container = document.getElementById('task-progress-container');
    if (!container) return;

    var tp = state.taskProgress;
    if (!tp.totalTasks) {
        container.classList.add('hidden');
        return;
    }

    container.classList.remove('hidden');

    var bar = document.getElementById('task-progress-bar-inner');
    var label = document.getElementById('task-progress-label');
    var list = document.getElementById('task-progress-task-list');

    if (bar) {
        var pct = tp.totalTasks > 0 ? (tp.tasks.filter(function(t) { return t.status === 'completed'; }).length / tp.totalTasks * 100) : 0;
        bar.style.width = pct + '%';
    }

    if (label) {
        label.textContent = 'Task ' + (tp.currentTask || '?') + ' / ' + tp.totalTasks;
    }

    if (list) {
        list.innerHTML = tp.tasks.map(function(t) {
            var statusClass = 'task-status-' + (t.status || 'pending');
            var icon = t.status === 'completed' ? '\u2713' : (t.status === 'failed' ? '\u2717' : (t.status === 'running' ? '\u25b6' : (t.status === 'paused' ? '\u23f8' : '\u25cb')));
            return '<div class="task-progress-item ' + statusClass + '">' +
                '<span class="task-item-icon">' + icon + '</span>' +
                '<span class="task-item-number">' + t.number + '</span>' +
                '<span class="task-item-title">' + escapeHtml(t.title || 'Task ' + t.number) + '</span>' +
                (t.error ? '<span class="task-item-error" title="' + escapeHtml(t.error) + '">!</span>' : '') +
                '</div>';
        }).join('');
    }
}

function showTaskReviewPopup(taskNumber, taskTitle, proposals) {
    var popup = document.getElementById('task-review-popup');
    if (!popup) return;

    popup.classList.remove('hidden');

    var titleEl = document.getElementById('task-review-title');
    if (titleEl) titleEl.textContent = 'Task ' + taskNumber + ': ' + (taskTitle || '');

    var list = document.getElementById('task-review-proposals');
    if (list) {
        list.innerHTML = proposals.map(function(p) {
            var kindLabel = p.kind === 'Create' ? 'CREATE' : (p.kind === 'Delete' ? 'DELETE' : 'MODIFY');
            return '<div class="task-review-file">' +
                '<span class="task-review-file-kind">' + kindLabel + '</span>' +
                '<span class="task-review-file-path">' + escapeHtml(p.file_path) + '</span>' +
                '</div>';
        }).join('');
    }
}

function closeTaskReviewPopup() {
    var popup = document.getElementById('task-review-popup');
    if (popup) popup.classList.add('hidden');
}

async function resumeBuild(action) {
    if (!state.taskProgress.runId) return;

    closeTaskReviewPopup();
    state.taskProgress.paused = false;
    state.taskProgress.pausedProposals = [];

    try {
        var result = await invoke('resume_build', {
            input: {
                run_id: state.taskProgress.runId,
                action: action
            }
        });
        addChatMessage('assistant', result);
    } catch (err) {
        addChatMessage('assistant', 'Error resuming build: ' + err);
    }
}


document.addEventListener('DOMContentLoaded', init);
