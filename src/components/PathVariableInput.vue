<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick, computed } from 'vue';
import { $t } from '../i18n';
import { commands } from '../bindings';
import { LAYER } from '../ui/layers';

type PathStatus = 'idle' | 'resolving' | 'ok' | 'not-found' | 'error';

const props = defineProps({
  modelValue: {
    type: String,
    default: '',
  },
  /** When true, show a debounced path-resolution status indicator below the editor.
   * @deprecated Use statusMode instead. */
  showStatus: {
    type: Boolean,
    default: false,
  },
  /** Controls how the path resolution status is displayed.
   * - 'below': inline status bar below the editor (default when showStatus=true)
   * - 'tooltip': compact status dot inside the editor with resolved path as tooltip
   * - 'none': no status display (default when showStatus=false) */
  statusMode: {
    type: String as () => 'below' | 'tooltip' | 'none',
    default: undefined,
  },
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const rootRef = ref<HTMLElement | null>(null);
const editorRef = ref<HTMLDivElement | null>(null);
const suggestionsRef = ref<HTMLElement | null>(null);
let savedCursorOffset = -1;

const VAR_RE = /<([a-zA-Z]+)>/g;

// ── Path variable definitions ──

interface PathVariable {
  name: string;
  labelKey: string;
  value: string;
}

const pathVariables: PathVariable[] = [
  { name: 'home', labelKey: 'home', value: '<home>' },
  { name: 'osUserName', labelKey: 'os_user_name', value: '<osUserName>' },
  { name: 'winAppData', labelKey: 'win_app_data', value: '<winAppData>' },
  { name: 'winLocalAppData', labelKey: 'win_local_app_data', value: '<winLocalAppData>' },
  {
    name: 'winLocalAppDataLow',
    labelKey: 'win_local_app_data_low',
    value: '<winLocalAppDataLow>',
  },
  { name: 'winDocuments', labelKey: 'win_documents', value: '<winDocuments>' },
  { name: 'winPublic', labelKey: 'win_public', value: '<winPublic>' },
  { name: 'winProgramData', labelKey: 'win_program_data', value: '<winProgramData>' },
  { name: 'winDir', labelKey: 'win_dir', value: '<winDir>' },
  { name: 'xdgData', labelKey: 'xdg_data', value: '<xdgData>' },
  { name: 'xdgConfig', labelKey: 'xdg_config', value: '<xdgConfig>' },
];

// ── Autocomplete state ──

const showSuggestions = ref(false);
const suggestionFilter = ref('');
const selectedIndex = ref(0);
const suggestionsStyle = ref<Record<string, string>>({});

const filteredVariables = computed(() => {
  const q = suggestionFilter.value.toLowerCase();
  if (!q) return pathVariables;
  return pathVariables.filter(
    (v) =>
      v.name.toLowerCase().includes(q) ||
      $t(`path_variable.${v.labelKey}`).toLowerCase().includes(q)
  );
});

const effectiveStatusMode = computed<'below' | 'tooltip' | 'none'>(() => {
  if (props.statusMode !== undefined) return props.statusMode;
  return props.showStatus ? 'below' : 'none';
});

// ── Parsing helpers ──

interface PathSegment {
  type: 'text' | 'variable';
  value: string;
}

function parsePath(path: string): PathSegment[] {
  const segments: PathSegment[] = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  VAR_RE.lastIndex = 0;
  while ((match = VAR_RE.exec(path)) !== null) {
    if (match.index > lastIndex) {
      segments.push({ type: 'text', value: path.slice(lastIndex, match.index) });
    }
    segments.push({ type: 'variable', value: match[0] });
    lastIndex = VAR_RE.lastIndex;
  }

  if (lastIndex < path.length) {
    segments.push({ type: 'text', value: path.slice(lastIndex) });
  }
  return segments;
}

function escapeHtml(text: string): string {
  const d = document.createElement('div');
  d.appendChild(document.createTextNode(text));
  return d.innerHTML;
}

function segmentsToHtml(segments: PathSegment[]): string {
  return segments
    .map((s) => {
      if (s.type === 'variable') {
        const escaped = escapeHtml(s.value);
        return `<span class="pvi-tag" contenteditable="false" data-var="${escaped}">${escaped}</span>`;
      }
      return escapeHtml(s.value);
    })
    .join('');
}

// ── DOM ↔ raw string ──

function nodeRawLen(node: Node): number {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent?.length || 0;
  if (node.nodeType === Node.ELEMENT_NODE) {
    const el = node as HTMLElement;
    if (el.classList.contains('pvi-tag')) return (el.dataset.var || '').length;
    return el.textContent?.length || 0;
  }
  return 0;
}

function extractPath(): string {
  if (!editorRef.value) return '';
  let result = '';
  for (const node of Array.from(editorRef.value.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE) {
      result += node.textContent || '';
    } else if (node.nodeType === Node.ELEMENT_NODE) {
      const el = node as HTMLElement;
      if (el.classList.contains('pvi-tag')) {
        result += el.dataset.var || el.textContent || '';
      } else {
        result += el.textContent || '';
      }
    }
  }
  return result;
}

function renderContent(path: string) {
  if (!editorRef.value) return;
  editorRef.value.innerHTML = segmentsToHtml(parsePath(path));
}

// ── Cursor offset helpers ──

function getCursorOffset(): number {
  if (!editorRef.value) return -1;
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return -1;
  const range = sel.getRangeAt(0);
  if (!editorRef.value.contains(range.startContainer)) return -1;

  let offset = 0;
  const children = Array.from(editorRef.value.childNodes);

  for (let i = 0; i < children.length; i++) {
    const node = children[i];
    if (!node) continue;
    if (range.startContainer === editorRef.value && i === range.startOffset) return offset;
    if (range.startContainer === node) return offset + range.startOffset;
    if (node.contains(range.startContainer)) return offset;
    offset += nodeRawLen(node);
  }

  return offset;
}

function setCursorAtOffset(targetOffset: number) {
  if (!editorRef.value) return;
  const sel = window.getSelection();
  if (!sel) return;

  let offset = 0;
  const children = Array.from(editorRef.value.childNodes);

  for (const node of children) {
    const len = nodeRawLen(node);
    if (offset + len >= targetOffset) {
      const r = document.createRange();
      if (node.nodeType === Node.TEXT_NODE) {
        const pos = Math.min(targetOffset - offset, node.textContent?.length || 0);
        r.setStart(node, pos);
      } else {
        r.setStartAfter(node);
      }
      r.collapse(true);
      sel.removeAllRanges();
      sel.addRange(r);
      return;
    }
    offset += len;
  }

  const r = document.createRange();
  r.selectNodeContents(editorRef.value);
  r.collapse(false);
  sel.removeAllRanges();
  sel.addRange(r);
}

// ── Autocomplete logic ──

function updateSuggestionsPosition() {
  if (!rootRef.value) return;
  const rect = rootRef.value.getBoundingClientRect();
  suggestionsStyle.value = {
    position: 'fixed',
    top: `${rect.bottom + 4}px`,
    left: `${rect.left}px`,
    width: `${rect.width}px`,
    zIndex: String(LAYER.PATH_AUTOCOMPLETE),
  };
}

function checkAutocomplete() {
  const raw = extractPath();
  const offset = getCursorOffset();
  if (offset < 0) {
    showSuggestions.value = false;
    return;
  }

  const beforeCursor = raw.slice(0, offset);
  const lastOpen = beforeCursor.lastIndexOf('<');
  const lastClose = beforeCursor.lastIndexOf('>');

  if (lastOpen >= 0 && lastOpen > lastClose) {
    suggestionFilter.value = beforeCursor.slice(lastOpen + 1);
    selectedIndex.value = 0;
    updateSuggestionsPosition();
    showSuggestions.value = true;
  } else {
    showSuggestions.value = false;
  }
}

function selectSuggestion(variable: PathVariable) {
  const raw = props.modelValue || '';
  let offset = getCursorOffset();
  if (offset < 0) offset = savedCursorOffset;
  if (offset < 0) return;

  const beforeCursor = raw.slice(0, offset);
  const lastOpen = beforeCursor.lastIndexOf('<');
  if (lastOpen < 0) return;

  const newRaw = raw.slice(0, lastOpen) + variable.value + raw.slice(offset);
  emit('update:modelValue', newRaw);
  showSuggestions.value = false;
  savedCursorOffset = -1;

  const newCursorPos = lastOpen + variable.value.length;
  nextTick(() => {
    renderContent(newRaw);
    setCursorAtOffset(newCursorPos);
    editorRef.value?.focus();
  });
}

function scrollSelectedIntoView() {
  nextTick(() => {
    const active = document.querySelector('.pvi-suggestion-item.active');
    if (active) active.scrollIntoView({ block: 'nearest' });
  });
}

// ── Event handlers ──

function onInput() {
  const raw = extractPath();
  emit('update:modelValue', raw);
  checkAutocomplete();

  // Real-time tag rendering when autocomplete is not active
  if (!showSuggestions.value) {
    const curOffset = getCursorOffset();
    nextTick(() => {
      renderContent(raw);
      if (curOffset >= 0) setCursorAtOffset(curOffset);
    });
  }
}

function onBlur() {
  savedCursorOffset = getCursorOffset();
  // Delay to allow mousedown on suggestion items to fire first
  setTimeout(() => {
    showSuggestions.value = false;
  }, 150);

  nextTick(() => {
    if (editorRef.value && !editorRef.value.contains(document.activeElement)) {
      renderContent(props.modelValue);
    }
  });
}

function onKeydown(e: KeyboardEvent) {
  // Backspace: atomically delete a variable tag when cursor is immediately after it
  if (e.key === 'Backspace' && !e.isComposing && !showSuggestions.value) {
    const sel = window.getSelection();
    if (sel && sel.rangeCount > 0 && editorRef.value) {
      const range = sel.getRangeAt(0);
      if (range.collapsed) {
        let nodeBefore: Node | null = null;
        if (range.startContainer === editorRef.value) {
          const idx = range.startOffset - 1;
          if (idx >= 0) nodeBefore = editorRef.value.childNodes[idx] ?? null;
        } else if (range.startOffset === 0) {
          nodeBefore = range.startContainer.previousSibling;
        }
        if (nodeBefore instanceof HTMLElement && nodeBefore.classList.contains('pvi-tag')) {
          e.preventDefault();
          nodeBefore.remove();
          const newRaw = extractPath();
          emit('update:modelValue', newRaw);
          return;
        }
      }
    }
  }
  if (showSuggestions.value && filteredVariables.value.length > 0) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIndex.value = (selectedIndex.value + 1) % filteredVariables.value.length;
      scrollSelectedIntoView();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIndex.value =
        (selectedIndex.value - 1 + filteredVariables.value.length) % filteredVariables.value.length;
      scrollSelectedIntoView();
      return;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      e.preventDefault();
      const v = filteredVariables.value[selectedIndex.value];
      if (v) selectSuggestion(v);
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      showSuggestions.value = false;
      return;
    }
  }
  if (e.key === 'Enter') e.preventDefault();
}

function onPaste(e: ClipboardEvent) {
  e.preventDefault();
  const text = e.clipboardData?.getData('text/plain') || '';
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return;
  const range = sel.getRangeAt(0);
  range.deleteContents();
  const textNode = document.createTextNode(text);
  range.insertNode(textNode);
  range.setStartAfter(textNode);
  range.setEndAfter(textNode);
  sel.removeAllRanges();
  sel.addRange(range);
  onInput();
}

// ── Public API ──

function insertAtCursor(variable: string) {
  if (!editorRef.value) return;

  const raw = props.modelValue || '';
  let offset: number;
  const cur = getCursorOffset();
  if (cur >= 0) {
    offset = cur;
  } else if (savedCursorOffset >= 0) {
    offset = savedCursorOffset;
  } else {
    offset = raw.length;
  }
  if (offset > raw.length) offset = raw.length;

  const newRaw = raw.slice(0, offset) + variable + raw.slice(offset);
  emit('update:modelValue', newRaw);
  savedCursorOffset = -1;

  nextTick(() => {
    renderContent(newRaw);
    setCursorAtOffset(offset + variable.length);
    editorRef.value?.focus();
  });
}

defineExpose({ insertAtCursor });

// ── Debounced path resolution ──

const pathStatus = ref<PathStatus>('idle');
const resolvedPathText = ref('');
let resolveTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleResolve(path: string) {
  if (effectiveStatusMode.value === 'none') return;
  if (resolveTimer) clearTimeout(resolveTimer);

  if (!path) {
    pathStatus.value = 'idle';
    resolvedPathText.value = '';
    return;
  }

  pathStatus.value = 'resolving';
  resolvedPathText.value = '';
  resolveTimer = setTimeout(async () => {
    try {
      const result = await commands.checkPaths([path]);
      // Guard against stale responses
      if (path !== props.modelValue) return;
      if (result.status === 'error') {
        resolvedPathText.value = result.error;
        pathStatus.value = 'error';
        return;
      }
      const check = result.data[0];
      if (!check) {
        pathStatus.value = 'error';
        return;
      }
      switch (check.status) {
        case 'ok':
          resolvedPathText.value = check.resolvedPath;
          pathStatus.value = 'ok';
          break;
        case 'notFound':
          resolvedPathText.value = check.resolvedPath + ' (' + $t('path_variable.not_found') + ')';
          pathStatus.value = 'not-found';
          break;
        case 'resolveFailed':
          resolvedPathText.value = check.error;
          pathStatus.value = 'error';
          break;
        case 'registryNotSupported':
          resolvedPathText.value = $t('path_variable.registry_not_supported');
          pathStatus.value = 'error';
          break;
      }
    } catch {
      if (path !== props.modelValue) return;
      pathStatus.value = 'error';
      resolvedPathText.value = '';
    }
  }, 500);
}

// ── Lifecycle ──

function onWindowScroll(e: Event) {
  // Don't close when scrolling inside the suggestions dropdown itself
  if (suggestionsRef.value && e.target instanceof Node && suggestionsRef.value.contains(e.target)) {
    return;
  }
  showSuggestions.value = false;
}

onMounted(() => {
  renderContent(props.modelValue);
  window.addEventListener('scroll', onWindowScroll, true);
  if (effectiveStatusMode.value !== 'none' && props.modelValue) scheduleResolve(props.modelValue);
});

onUnmounted(() => {
  window.removeEventListener('scroll', onWindowScroll, true);
  if (resolveTimer) clearTimeout(resolveTimer);
});

watch(
  () => props.modelValue,
  (newVal) => {
    if (newVal !== extractPath()) {
      renderContent(newVal);
    }
    scheduleResolve(newVal);
  }
);
</script>

<template>
  <div class="pvi-wrapper">
    <div ref="rootRef" class="pvi-root">
      <div class="pvi-editor-area">
        <div
          ref="editorRef"
          class="pvi-editor"
          :class="{ 'pvi-editor--with-badge': !$slots.append }"
          :data-placeholder="$t('path_variable.placeholder')"
          contenteditable="true"
          spellcheck="false"
          @input="onInput"
          @blur="onBlur"
          @keydown="onKeydown"
          @paste="onPaste"
        />
        <el-tooltip
          v-if="!$slots.append"
          :content="$t('path_variable.editor_badge_tooltip')"
          placement="top"
          :show-after="200"
        >
          <span class="pvi-editor-badge">&lt;/&gt;</span>
        </el-tooltip>
      </div>
      <!-- Compact status dot for tooltip mode -->
      <el-tooltip
        v-if="effectiveStatusMode === 'tooltip' && modelValue"
        :content="resolvedPathText || $t('path_variable.resolving')"
        placement="top"
        :show-after="200"
      >
        <span class="pvi-status-dot-compact" :class="`pvi-status--${pathStatus}`" />
      </el-tooltip>
      <div v-if="$slots.append" class="pvi-append">
        <slot name="append" :insert-at-cursor="insertAtCursor" />
      </div>
    </div>
    <div
      v-if="effectiveStatusMode === 'below' && modelValue"
      class="pvi-status"
      :class="`pvi-status--${pathStatus}`"
    >
      <span class="pvi-status-dot" />
      <span class="pvi-status-text">{{ resolvedPathText }}</span>
    </div>
  </div>

  <!-- Teleport to body to avoid overflow clipping in table cells -->
  <Teleport to="body">
    <Transition name="pvi-dropdown">
      <div
        v-if="showSuggestions && filteredVariables.length > 0"
        ref="suggestionsRef"
        class="pvi-suggestions"
        :style="suggestionsStyle"
      >
        <div
          v-for="(v, i) in filteredVariables"
          :key="v.name"
          class="pvi-suggestion-item"
          :class="{ active: i === selectedIndex }"
          @mousedown.prevent="selectSuggestion(v)"
        >
          <span class="pvi-suggestion-var">{{ v.value }}</span>
          <span class="pvi-suggestion-label">{{ $t(`path_variable.${v.labelKey}`) }}</span>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<!-- Non-scoped styles for dynamic tag elements and teleported dropdown -->
<style>
.pvi-tag {
  display: inline-flex;
  align-items: center;
  background: var(--el-color-warning-light-9);
  color: var(--el-color-warning-dark-2);
  border-radius: 4px;
  border: 1px solid var(--el-color-warning-light-5);
  padding: 0 6px;
  height: 20px;
  margin: 0 1px;
  font-family: inherit;
  font-size: 11px;
  line-height: 1;
  vertical-align: middle;
  user-select: all;
  cursor: default;
}

.pvi-suggestions {
  background: var(--el-bg-color-overlay);
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--el-border-radius-base);
  box-shadow: var(--el-box-shadow-light);
  max-height: 200px;
  overflow-y: auto;
}

.pvi-suggestion-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 12px;
  cursor: pointer;
  transition: background 0.15s;
}

.pvi-suggestion-item:hover,
.pvi-suggestion-item.active {
  background: var(--el-fill-color-light);
}

.pvi-suggestion-var {
  display: inline-block;
  background: var(--el-color-warning-light-9);
  color: var(--el-color-warning-dark-2);
  border-radius: 3px;
  padding: 0 6px;
  font-size: 12px;
  line-height: 20px;
  flex-shrink: 0;
}

.pvi-suggestion-label {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pvi-dropdown-enter-active,
.pvi-dropdown-leave-active {
  transition:
    opacity 0.15s,
    transform 0.15s;
}

.pvi-dropdown-enter-from,
.pvi-dropdown-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>

<style scoped>
.pvi-root {
  position: relative;
  display: inline-flex;
  align-items: stretch;
  width: 100%;
  min-height: 34px;
  box-shadow: 0 0 0 1px var(--el-border-color) inset;
  border-radius: var(--el-border-radius-base);
  background: var(--el-fill-color-blank);
  transition: box-shadow 0.2s;
  font-size: 12px;
  line-height: 24px;
  box-sizing: border-box;
}

.pvi-root:hover {
  box-shadow: 0 0 0 1px var(--el-border-color-hover) inset;
}

.pvi-root:focus-within {
  box-shadow:
    0 0 0 1px var(--el-color-primary) inset,
    0 0 0 3px var(--el-color-primary-light-8);
}

.pvi-wrapper {
  width: 100%;
  min-width: 0;
}

.pvi-editor-area {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  position: relative;
  display: flex;
  align-items: center;
}

.pvi-editor {
  flex: 1;
  min-width: 0;
  min-height: 24px;
  padding: 4px 8px;
  outline: none;
  line-height: 22px;
  white-space: nowrap;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
  color: var(--el-text-color-regular);
}

.pvi-editor--with-badge {
  padding-right: 32px;
}

.pvi-editor-badge {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--el-text-color-placeholder);
  font-size: 10px;
  font-family: monospace;
  pointer-events: auto;
  user-select: none;
  cursor: default;
}

.pvi-editor::-webkit-scrollbar {
  display: none;
}

.pvi-append {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0 4px 0 0;
  background: transparent;
  border-left: none;
  border-radius: 0 var(--el-border-radius-base) var(--el-border-radius-base) 0;
}

/* Compact status dot inside the editor (tooltip mode) */
.pvi-status-dot-compact {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
  margin: auto 6px;
  cursor: default;
  background: var(--el-color-info-light-5);
  transition: background 0.2s;
}

.pvi-status-dot-compact.pvi-status--resolving {
  animation: pvi-pulse 1s infinite;
}

.pvi-status-dot-compact.pvi-status--ok {
  background: var(--el-color-success);
}

.pvi-status-dot-compact.pvi-status--not-found,
.pvi-status-dot-compact.pvi-status--error {
  background: var(--el-color-danger);
}

/* Placeholder text via pseudo-element */
.pvi-editor:empty::before {
  content: attr(data-placeholder);
  color: var(--el-text-color-placeholder);
  pointer-events: none;
  white-space: nowrap;
}

.pvi-status {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-top: 4px;
  font-size: 11px;
  line-height: 16px;
  min-height: 16px;
}

.pvi-status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.pvi-status--idle .pvi-status-dot {
  background: var(--el-color-info-light-5);
}

.pvi-status--resolving .pvi-status-dot {
  background: var(--el-color-info-light-5);
  animation: pvi-pulse 1s infinite;
}

.pvi-status--ok .pvi-status-dot {
  background: var(--el-color-success);
}

.pvi-status--not-found .pvi-status-dot,
.pvi-status--error .pvi-status-dot {
  background: var(--el-color-danger);
}

.pvi-status-text {
  color: var(--el-text-color-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.pvi-status--error .pvi-status-text {
  color: var(--el-color-danger);
}

@keyframes pvi-pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}
</style>
