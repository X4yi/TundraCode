function showToolActivity(payload) {
    var container = document.getElementById('chat-messages');
    if (!container) return;
    var existing = container.querySelector('.tool-activity[data-call-id="' + payload.call_id + '"]');
    if (existing) {
        existing.classList.remove('running');
        existing.classList.add('done');
        var statusEl = existing.querySelector('.tool-status');
        if (statusEl) statusEl.textContent = '\u2713';
        var durEl = existing.querySelector('.tool-duration');
        if (durEl && payload.duration_ms) {
            durEl.textContent = (payload.duration_ms / 1000).toFixed(1) + 's';
        }
        return;
    }
    var template = getToolTemplate(payload.tool_name);
    var args = payload.arguments || {};
    var displayText = template.running(args);
    var fileChip = payload.file_path
        ? ' <span class="tool-file">' + escapeHtml(payload.file_path) + '</span>'
        : '';
    var activity = document.createElement('div');
    activity.className = 'tool-activity running';
    activity.dataset.callId = payload.call_id;
    activity.innerHTML = '<span class="tool-icon">' + template.icon + '</span>' +
        ' <span class="tool-name">' + template.label + '</span>' +
        '<span class="tool-detail"> ' + escapeHtml(displayText) + '</span>' +
        fileChip +
        ' <span class="tool-status running">\u23F3</span>';
    container.appendChild(activity);
    container.scrollTop = container.scrollHeight;
}

function addToolLogEntry(toolName, filePath, callId, status, details) {
    var emptyState = document.getElementById('tool-log-empty');
    var entriesEl = document.getElementById('tool-log-entries');
    var subpanel = document.getElementById('tool-log-subpanel');
    var toolbar = document.getElementById('tool-log-toolbar');
    if (emptyState) emptyState.style.display = 'none';
    if (entriesEl) entriesEl.style.display = 'block';
    if (subpanel && state.toolLogVisible) subpanel.classList.remove('collapsed');
    if (toolbar) toolbar.classList.remove('hidden');
    if (state.toolCalls.length === 0 || state.toolCalls.length === 1) populateToolFilter();

    var template = getToolTemplate(toolName);
    var statusClass = status === 'running' ? 'running' : (status === 'done' ? 'done' : 'error');
    var existingCall = state.toolCalls.find(function(c) { return c.callId === callId; });
    var startTime = existingCall ? existingCall.startTime : Date.now();
    var duration = '';
    if (status === 'done' && existingCall && existingCall.startTime) {
        duration = ((Date.now() - existingCall.startTime) / 1000).toFixed(1) + 's';
    }
    var time = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
    var displayText = '';
    if (status === 'running') {
        var args = details && details.arguments ? details.arguments : {};
        displayText = template.running(args);
    } else if (status === 'done') {
        displayText = template.complete(details && details.resultSummary ? details.resultSummary : '', filePath);
    } else {
        displayText = toolName + ' failed';
    }
    var statusHtml = '';
    if (status === 'running') {
        statusHtml = '<span class="tool-log-spinner"></span><span>Running...</span>';
    } else if (status === 'done') {
        statusHtml = '<span class="tool-log-check">\u2713</span>' + (duration ? '<span class="tool-log-duration">' + duration + '</span>' : '');
    } else {
        statusHtml = '<span>Error</span>';
    }
    var entry = document.createElement('div');
    entry.className = 'tool-log-entry';
    entry.dataset.callId = callId;
    entry.dataset.toolName = toolName;
    entry.innerHTML =
        '<div class="tool-log-entry-header">' +
            '<div class="tool-log-entry-tool">' +
                '<span class="tool-log-entry-tool-icon">' + template.icon + '</span>' +
                '<span>' + displayText + '</span>' +
            '</div>' +
            '<div class="tool-log-entry-status ' + statusClass + '">' +
                statusHtml +
            '</div>' +
            '<span class="tool-log-entry-time">' + time + '</span>' +
        '</div>' +
        (filePath ? '<div class="tool-log-entry-file">' + escapeHtml(filePath) + '</div>' : '') +
        '<div class="tool-log-entry-expand" aria-label="Expand">\u25B8</div>';
    var expandBtn = entry.querySelector('.tool-log-entry-expand');
    if (expandBtn) {
        expandBtn.addEventListener('click', function() {
            entry.classList.toggle('expanded');
        });
    }
    if (entriesEl) {
        entriesEl.appendChild(entry);
        entriesEl.scrollTop = entriesEl.scrollHeight;
    }
    updateRunningStates();
    if (state.toolCalls.length > 100) {
        state.toolCalls.shift();
        var firstEntry = entriesEl.querySelector('.tool-log-entry');
        if (firstEntry) firstEntry.remove();
    }
}

function updateToolLogEntry(callId, status, details) {
    var entriesEl = document.getElementById('tool-log-entries');
    if (!entriesEl) return;
    var entry = entriesEl.querySelector('.tool-log-entry[data-call-id="' + callId + '"]');
    if (!entry) return;
    var existingCall = state.toolCalls.find(function(c) { return c.callId === callId; });
    var toolName = entry.dataset.toolName || (existingCall ? existingCall.toolName : '');
    var template = getToolTemplate(toolName);
    var startTime = existingCall ? existingCall.startTime : null;
    var duration = '';
    if (status === 'done' && startTime) {
        duration = ((Date.now() - startTime) / 1000).toFixed(1) + 's';
    }
    var statusEl = entry.querySelector('.tool-log-entry-status');
    if (statusEl) {
        statusEl.className = 'tool-log-entry-status ' + (status === 'running' ? 'running' : (status === 'done' ? 'done' : 'error'));
        var statusHtml = '';
        if (status === 'running') {
            statusHtml = '<span class="tool-log-spinner"></span><span>Running...</span>';
        } else if (status === 'done') {
            statusHtml = '<span class="tool-log-check">\u2713</span>' + (duration ? '<span class="tool-log-duration">' + duration + '</span>' : '');
        } else {
            statusHtml = '<span>Error</span>';
        }
        statusEl.innerHTML = statusHtml;
    }
    var toolTextEl = entry.querySelector('.tool-log-entry-tool span:last-child');
    if (toolTextEl && status === 'done') {
        var fp = existingCall ? existingCall.filePath : null;
        toolTextEl.textContent = template.complete(details && details.resultSummary ? details.resultSummary : '', fp);
    }
    if (details && details.outputPreview) {
        var detailsEl = entry.querySelector('.tool-log-entry-details');
        if (!detailsEl) {
            detailsEl = document.createElement('div');
            detailsEl.className = 'tool-log-entry-details';
            entry.appendChild(detailsEl);
        }
        detailsEl.textContent = details.outputPreview;
    }
    updateRunningStates();
}

function updateRunningStates() {
    var entriesEl = document.getElementById('tool-log-entries');
    if (!entriesEl) return;
    var allEntries = entriesEl.querySelectorAll('.tool-log-entry');
    var runningEntries = Array.from(allEntries).filter(function(el) {
        return el.querySelector('.tool-log-entry-status.running');
    });
    runningEntries.forEach(function(entry, index) {
        var statusEl = entry.querySelector('.tool-log-entry-status');
        var spinner = statusEl.querySelector('.tool-log-spinner');
        var isLast = index === runningEntries.length - 1;
        if (spinner) spinner.style.display = isLast ? 'inline-block' : 'none';
    });
}

function populateToolFilter() {
    var filter = document.getElementById('tool-log-filter');
    if (!filter) return;
    var tools = Object.keys(TOOL_TEMPLATES);
    var currentValue = filter.value;
    filter.innerHTML = '<option value="">All Tools</option>';
    tools.forEach(function(tool) {
        var tpl = TOOL_TEMPLATES[tool];
        var option = document.createElement('option');
        option.value = tool;
        option.textContent = tpl.label;
        filter.appendChild(option);
    });
    filter.value = currentValue;
}

function clearToolLog(e) {
    if (e) e.stopPropagation();
    state.toolCalls = [];
    var entriesEl = document.getElementById('tool-log-entries');
    var emptyState = document.getElementById('tool-log-empty');
    if (entriesEl) { entriesEl.innerHTML = ''; entriesEl.style.display = 'none'; }
    if (emptyState) emptyState.style.display = 'flex';
    state.tokenBreakdown = { input: 0, output: 0, total: 0 };
    state.lastRequestTokens = 0;
    updateTokenDisplay();
}


