<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick, computed } from 'vue';
import { $t } from '../i18n';

const props = defineProps({
  modelValue: {
    type: String,
    default: '',
  },
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const rootRef = ref<HTMLElement | null>(null);
const editorRef = ref<HTMLDivElement | null>(null);
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
      $t(`path_variable.${v.labelKey}`).toLowerCase().includes(q),
  );
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
    zIndex: '2050',
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
        (selectedIndex.value - 1 + filteredVariables.value.length) %
        filteredVariables.value.length;
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

// ── Lifecycle ──

function onWindowScroll() {
  showSuggestions.value = false;
}

onMounted(() => {
  renderContent(props.modelValue);
  window.addEventListener('scroll', onWindowScroll, true);
});

onUnmounted(() => {
  window.removeEventListener('scroll', onWindowScroll, true);
});

watch(
  () => props.modelValue,
  (newVal) => {
    if (newVal !== extractPath()) {
      renderContent(newVal);
    }
  },
);
</script>

<template>
  <div ref="rootRef" class="pvi-root">
    <div class="pvi-editor-area">
      <div
        ref="editorRef"
        class="pvi-editor"
        contenteditable="true"
        spellcheck="false"
        @input="onInput"
        @blur="onBlur"
        @keydown="onKeydown"
        @paste="onPaste"
      />
    </div>
    <div v-if="$slots.append" class="pvi-append">
      <slot name="append" :insert-at-cursor="insertAtCursor" />
    </div>
  </div>

  <!-- Teleport to body to avoid overflow clipping in table cells -->
  <Teleport to="body">
    <Transition name="pvi-dropdown">
      <div
        v-if="showSuggestions && filteredVariables.length > 0"
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
  display: inline-block;
  background: var(--el-color-warning-light-9);
  color: var(--el-color-warning-dark-2);
  border-radius: 3px;
  padding: 0 4px;
  margin: 0 1px;
  font-family: inherit;
  font-size: inherit;
  line-height: inherit;
  vertical-align: baseline;
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
  box-shadow: 0 0 0 1px var(--el-color-primary) inset;
}

.pvi-editor-area {
  flex: 1;
  min-width: 0;
  overflow: hidden;
}

.pvi-editor {
  min-height: 22px;
  padding: 0 8px;
  outline: none;
  white-space: nowrap;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
  color: var(--el-text-color-regular);
}

.pvi-editor::-webkit-scrollbar {
  display: none;
}

.pvi-append {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  padding: 0 12px;
  background: var(--el-fill-color-light);
  border-left: 1px solid var(--el-border-color);
  border-radius: 0 var(--el-border-radius-base) var(--el-border-radius-base) 0;
}
</style>
