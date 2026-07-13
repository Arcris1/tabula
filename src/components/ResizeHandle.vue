<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  modelValue: number;
  axis: "x" | "y";        // x = resize width (col-resize), y = resize height (row-resize)
  min?: number;
  max?: number;
  invert?: boolean;       // true when the resized panel sits to the right/below the handle
}>();
const emit = defineEmits<{ "update:modelValue": [n: number] }>();

const dragging = ref(false);
let startPos = 0;
let startVal = 0;

function onDown(e: PointerEvent) {
  dragging.value = true;
  startPos = props.axis === "x" ? e.clientX : e.clientY;
  startVal = props.modelValue;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  e.preventDefault();
}
function onMove(e: PointerEvent) {
  if (!dragging.value) return;
  const cur = props.axis === "x" ? e.clientX : e.clientY;
  let delta = cur - startPos;
  if (props.invert) delta = -delta;
  let next = startVal + delta;
  if (props.min != null) next = Math.max(props.min, next);
  if (props.max != null) next = Math.min(props.max, next);
  emit("update:modelValue", Math.round(next));
}
function onUp(e: PointerEvent) {
  dragging.value = false;
  (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
}
</script>

<template>
  <div
    class="shrink-0 z-20 transition-colors"
    :class="[
      axis === 'x' ? 'w-1 -mx-0.5 cursor-col-resize self-stretch' : 'h-1 -my-0.5 cursor-row-resize',
      dragging ? 'bg-blue-500' : 'bg-transparent hover:bg-blue-500/60',
    ]"
    @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp" @pointercancel="onUp"
  />
</template>
