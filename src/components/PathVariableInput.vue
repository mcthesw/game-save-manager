<script setup lang="ts">
import { ref, watch, onMounted, nextTick } from 'vue';

const props = defineProps({
  modelValue: {
    type: String,
    default: '',
  },
});

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
}>();

const editorRef = ref<HTMLDivElement | null>(null);
// Cursor offset stored as a position in the raw string (survives re-renders)
let savedCursorOffset = -1;

// Regex for matching path variables like <home>, <winAppData>, etc.
const VAR_RE = /<([a-zA-Z]+)>/g;

// ---------- parsing helpers ----------

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

// ---------- DOM ↔ raw string ----------

/** Walk editor children and compute the raw-string length contribution of each node. */
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

// ---------- cursor offset helpers ----------

/** Convert the current DOM selection position to an offset in the raw string. */
function getCursorOffset(): number {
  if (!editorRef.value) return -1;
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return -1;
  const range = sel.getRangeAt(0);
  if (!editorRef.value.contains(range.startContainer)) return -1;

  let offset = 0;
  const children = Array.from(editorRef.value.childNodes);

  for (let i = 0; i < children.length; i++) {
    const node = children[i]!;

    // When startContainer is the editor itself, startOffset is the child index
    if (range.startContainer === editorRef.value && i === range.startOffset) return offset;

    // Cursor inside this text node
    if (range.startContainer === node) return offset + range.startOffset;

    // Cursor inside a child element (rare for contenteditable="false" spans)
    if (node.contains(range.startContainer)) return offset;

    offset += nodeRawLen(node);
  }

  // Cursor at the very end
  return offset;
}

/** Set the browser selection to the position corresponding to a raw-string offset. */
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
        r.setStart(node, targetOffset - offset);
      } else {
        // Element node (tag) – place cursor right after it
        r.setStartAfter(node);
      }
      r.collapse(true);
      sel.removeAllRanges();
      sel.addRange(r);
      return;
    }

    offset += len;
  }

  // Fallback: place at end
  const r = document.createRange();
  r.selectNodeContents(editorRef.value);
  r.collapse(false);
  sel.removeAllRanges();
  sel.addRange(r);
}

// ---------- event handlers ----------

function onInput() {
  emit('update:modelValue', extractPath());
}

function onBlur() {
  // Save cursor offset (survives DOM re-renders)
  savedCursorOffset = getCursorOffset();

  // Re-render so manually-typed variables become tags
  nextTick(() => {
    if (editorRef.value && !editorRef.value.contains(document.activeElement)) {
      renderContent(props.modelValue);
    }
  });
}

function onKeydown(e: KeyboardEvent) {
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

// ---------- public API ----------

function insertAtCursor(variable: string) {
  if (!editorRef.value) return;

  const raw = props.modelValue || '';

  // Determine insertion offset in the raw string
  let offset: number;
  const cur = getCursorOffset();
  if (cur >= 0) {
    offset = cur;
  } else if (savedCursorOffset >= 0) {
    offset = savedCursorOffset;
  } else {
    offset = raw.length;
  }

  if (offset < 0 || offset > raw.length) offset = raw.length;

  const newRaw = raw.slice(0, offset) + variable + raw.slice(offset);
  emit('update:modelValue', newRaw);
  savedCursorOffset = -1;

  // Re-render and restore cursor after the inserted variable
  nextTick(() => {
    renderContent(newRaw);
    setCursorAtOffset(offset + variable.length);
    editorRef.value?.focus();
  });
}

defineExpose({ insertAtCursor });

// ---------- reactivity ----------

watch(
  () => props.modelValue,
  (newVal) => {
    if (newVal !== extractPath()) {
      renderContent(newVal);
    }
  },
);

onMounted(() => {
  renderContent(props.modelValue);
});
</script>

<template>
  <div class="pvi-root">
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
</template>

<!-- Global (non-scoped) styles for dynamically-created tags inside contenteditable -->
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
</style>

<style scoped>
.pvi-root {
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
  color: var(--el-text-color-regular);
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
