import assert from 'node:assert/strict';
import test from 'node:test';
import { effectScope, nextTick, ref } from 'vue';
import { isActivityFeedbackVisible, overlayDepth, useOverlayDepth } from './overlayDepth.ts';

test('overlays hide the idle activity shortcut but not newly expanded feedback', () => {
  assert.equal(isActivityFeedbackVisible(0, false), true);
  assert.equal(isActivityFeedbackVisible(1, false), false);
  assert.equal(isActivityFeedbackVisible(1, true), true);
  assert.equal(isActivityFeedbackVisible(2, true), true);
});

test('nested overlay counts remain balanced on cancel and unmount', async () => {
  const scope = effectScope();
  const drawer = ref(true);
  const dialog = ref(false);
  scope.run(() => {
    useOverlayDepth(drawer);
    useOverlayDepth(dialog);
  });
  try {
    assert.equal(overlayDepth.value, 1);
    dialog.value = true;
    await nextTick();
    assert.equal(overlayDepth.value, 2);
    dialog.value = false;
    await nextTick();
    assert.equal(overlayDepth.value, 1);
  } finally {
    scope.stop();
  }
  assert.equal(overlayDepth.value, 0);
});
