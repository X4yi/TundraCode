function tp(args) { return args.p || args.path || ''; }
function tq(args) { return args.q || args.query || ''; }
function tc(args) { return args.c || args.command || ''; }
function targs(args) { return (args.a || args.args || []).join(' '); }

var TOOL_TEMPLATES = {
  ReadFile: {
    icon: '📖',
    label: 'Reading',
    running: function(args) {
      var p = tp(args);
      return p ? 'Reading: ' + p : 'Reading file';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return filePath ? '✓ Read ' + filePath : '✓ Read file';
    },
    details: function(result) { return { type: 'file', path: result.file_path, preview: result.output_preview }; }
  },
  WriteFile: {
    icon: '✏️',
    label: 'Writing',
    running: function(args) {
      var p = tp(args);
      return p ? 'Writing: ' + p : 'Writing file';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return filePath ? '✓ Wrote ' + filePath : '✓ Wrote file';
    },
    details: function(result) { return { type: 'file', path: result.file_path, preview: result.output_preview }; }
  },
  CreateFile: {
    icon: '📄',
    label: 'Creating',
    running: function(args) {
      var p = tp(args);
      return p ? 'Creating: ' + p : 'Creating file';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return filePath ? '✓ Created ' + filePath : '✓ Created file';
    },
    details: function(result) { return { type: 'file', path: result.file_path, preview: result.output_preview }; }
  },
  DeleteFile: {
    icon: '🗑️',
    label: 'Deleting',
    running: function(args) {
      var p = tp(args);
      return p ? 'Deleting: ' + p : 'Deleting file';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return filePath ? '✓ Deleted ' + filePath : '✓ Deleted file';
    },
    details: function(result) { return { type: 'file', path: result.file_path }; }
  },
  ApplyPatch: {
    icon: '🔧',
    label: 'Patching',
    running: function(args) {
      var p = tp(args);
      return p ? 'Patching: ' + p : 'Patching file';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return filePath ? '✓ Patched ' + filePath : '✓ Patched file';
    },
    details: function(result) { return { type: 'file', path: result.file_path, preview: result.output_preview }; }
  },
  ListDirectory: {
    icon: '📁',
    label: 'Listing',
    running: function(args) {
      var p = tp(args) || '.';
      return 'Listing: ' + p;
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return filePath ? '✓ Listed ' + filePath : '✓ Listed directory';
    },
    details: function(result) { return { type: 'directory', path: result.file_path, preview: result.output_preview }; }
  },
  GetWorkspace: {
    icon: '📂',
    label: 'Scanning',
    running: function(args) { return 'Scanning workspace...'; },
    complete: function(summary, filePath) { return '✓ Workspace scanned'; },
    details: function(result) { return { type: 'workspace', preview: result.output_preview }; }
  },
  RunCommand: {
    icon: '⚡',
    label: 'Running',
    running: function(args) {
      var c = tc(args);
      var a = targs(args);
      if (c && a) return 'Running: ' + c + ' ' + a;
      if (c) return 'Running: ' + c;
      return 'Running command';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return '✓ Ran command';
    },
    details: function(result) { return { type: 'command', cmd: result.cmd, exit_code: result.code, stdout: result.stdout, stderr: result.stderr }; }
  },
  GetDiagnostics: {
    icon: '🔍',
    label: 'Checking',
    running: function(args) {
      var f = tp(args) || args.f || args.file_path || '';
      return f ? 'Checking: ' + f : 'Checking diagnostics';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return '✓ Diagnostics done';
    },
    details: function(result) { return { type: 'diagnostics', path: result.file_path, preview: result.output_preview }; }
  },
  SearchCodebase: {
    icon: '🔎',
    label: 'Searching',
    running: function(args) {
      var q = tq(args);
      var g = args.g || args.file_pattern || '';
      if (q && g) return 'Searching: "' + q + '" in ' + g;
      if (q) return 'Searching: "' + q + '"';
      return 'Searching codebase';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return '✓ Search done';
    },
    details: function(result) { return { type: 'search', query: result.query, preview: result.output_preview }; }
  },
  SearchInWeb: {
    icon: '🌐',
    label: 'Researching',
    running: function(args) {
      var q = tq(args);
      return q ? 'Web search: "' + q + '"' : 'Web search';
    },
    complete: function(summary, filePath) {
      if (summary) return summary;
      return '✓ Web search done';
    },
    details: function(result) { return { type: 'web', query: result.query, preview: result.output_preview }; }
  },
};

var DEFAULT_TEMPLATE = {
  icon: '⚙️',
  label: 'Running',
  running: function(args) { return 'Running tool...'; },
  complete: function(summary, filePath) { return '✓ Tool done'; },
  details: function(result) { return { type: 'text', preview: result.output_preview }; }
};

function getToolTemplate(toolName) {
  return TOOL_TEMPLATES[toolName] || DEFAULT_TEMPLATE;
}
