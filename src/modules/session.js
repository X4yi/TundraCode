var SESSION_MANAGEMENT = {
    setup: function() {
        var btn = document.getElementById('session-selector-btn');
        var dropdown = document.getElementById('session-dropdown');

        btn.addEventListener('click', function(e) {
            e.stopPropagation();
            dropdown.classList.toggle('hidden');
            if (!dropdown.classList.contains('hidden')) {
                var rect = btn.getBoundingClientRect();
                var agentsPanel = document.getElementById('agents-panel');
                var pr = agentsPanel ? agentsPanel.getBoundingClientRect() : null;
                var leftPos = rect.left;
                if (pr) {
                    leftPos = Math.min(leftPos, pr.right - 300);
                    if (leftPos < pr.left) leftPos = pr.left;
                }
                dropdown.style.setProperty('--dd-left', leftPos + 'px');
                dropdown.style.setProperty('--dd-top', rect.bottom + 'px');
                SESSION_MANAGEMENT.refreshList();
            }
        });

        document.addEventListener('click', function(e) {
            if (!dropdown.contains(e.target) && !btn.contains(e.target)) {
                dropdown.classList.add('hidden');
            }
        });

        document.getElementById('session-new-btn').addEventListener('click', function() {
            newSession();
            dropdown.classList.add('hidden');
        });
    },

    updateSessionTitleDisplay: function() {
        var el = document.getElementById('session-title-display');
        if (!el) return;
        if (state.sessionTitle) {
            el.textContent = state.sessionTitle.length > 25
                ? state.sessionTitle.substring(0, 25) + '\u2026'
                : state.sessionTitle;
        } else {
            el.textContent = 'Session';
        }
    },

    async refreshList() {
        var list = document.getElementById('session-list');
        try {
            var sessions = await invoke('list_sessions');
            if (!sessions || sessions.length === 0) {
                list.innerHTML = '<div class="session-empty">No saved sessions</div>';
                return;
            }
            list.innerHTML = '';
            sessions.forEach(function(s) {
                var item = document.createElement('div');
                item.className = 'session-item';
                var info = document.createElement('div');
                info.className = 'session-item-info';
                var title = document.createElement('div');
                title.className = 'session-item-title';
                title.textContent = s.title || s.filename;
                var meta = document.createElement('div');
                meta.className = 'session-item-meta';
                var dateStr = s.date ? s.date.split('T')[0] : '';
                var modelStr = s.model || '';
                meta.textContent = [dateStr, modelStr].filter(Boolean).join(' | ');
                info.appendChild(title);
                info.appendChild(meta);
                var delBtn = document.createElement('button');
                delBtn.className = 'session-item-delete';
                delBtn.innerHTML = '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>';
                delBtn.addEventListener('click', async function(e) {
                    e.stopPropagation();
                    if (confirm('Delete session "' + (s.title || s.filename) + '"?')) {
                        await invoke('delete_session', { filename: s.filename });
                        SESSION_MANAGEMENT.refreshList();
                    }
                });
                item.appendChild(info);
                item.appendChild(delBtn);
                item.addEventListener('click', function() {
                    SESSION_MANAGEMENT.loadSession(s.filename);
                    document.getElementById('session-dropdown').classList.add('hidden');
                });
                list.appendChild(item);
            });
        } catch (e) {
            list.innerHTML = '<div class="session-empty">Error loading sessions</div>';
        }
    },

    async loadSession(filename) {
        try {
            var session = await invoke('load_session', { filename: filename });
            if (!session) return;
            newSession();
            state.currentSessionFile = filename;
            state.sessionTitle = session.title || null;
            SESSION_MANAGEMENT.updateSessionTitleDisplay();
            if (session.model) {
                var parts = session.model.split(' / ');
                if (parts.length === 2) {
                    state.selectedProviderId = parts[0];
                    state.selectedModelId = parts[1];
                    updateModelSelectorDisplay();
                    updateReasoningSelectorVisibility();
                }
            }
            var historyJson = await invoke('load_session_history', { filename: filename });
            if (historyJson && Array.isArray(historyJson)) {
                state.conversationHistory = historyJson;
                historyJson.forEach(function(msg) {
                    addChatMessage(msg.role, msg.content);
                });
            }
        } catch (e) {
            console.error('Error loading session:', e);
        }
    },

    async saveSession(title) {
        if (state.conversationHistory.length === 0) return null;
        var model = state.selectedProviderId && state.selectedModelId
            ? state.selectedProviderId + ' / ' + state.selectedModelId
            : '';
        try {
            var filename = await invoke('save_session', {
                title: title,
                historyJson: JSON.stringify(state.conversationHistory),
                model: model,
                tokens: state.tokenUsage.session,
                mode: state.agentMode,
            });
            state.currentSessionFile = filename;
            state.sessionTitle = title;
            SESSION_MANAGEMENT.updateSessionTitleDisplay();
            return filename;
        } catch (e) {
            console.error('Error saving session:', e);
            return null;
        }
    },
};

async function newSession() {
    if (state.conversationHistory.length > 0) {
        var firstUserMsg = state.conversationHistory.find(function(m) { return m.role === 'user'; });
        var title = firstUserMsg ? firstUserMsg.content.substring(0, 50) : 'Untitled session';
        await SESSION_MANAGEMENT.saveSession(title);
    }
    state.conversationHistory = [];
    state.tokenUsage = { session: 0, total: 0 };
    state.tokenBreakdown = { input: 0, output: 0, total: 0 };
    state.lastRequestTokens = 0;
    state.toolCalls = [];
    state.sessionTitle = null;
    state.currentSessionFile = null;
    state.pendingDiffs = {};
    resetReasoningBlock();
    updateTokenDisplay();
    updateTokenUsage(0);
    clearToolLog(null);
    var container = document.getElementById('chat-messages');
    if (container) container.innerHTML = '';
    var diffBadge = document.getElementById('diff-count-badge');
    if (diffBadge) { diffBadge.classList.add('hidden'); diffBadge.textContent = '0'; }
    var diffBtn = document.getElementById('diff-toggle-btn');
    if (diffBtn) diffBtn.style.display = 'none';
    SESSION_MANAGEMENT.updateSessionTitleDisplay();
}
