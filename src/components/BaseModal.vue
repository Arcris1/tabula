<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

const props = withDefaults(defineProps<{
  /** modal width scale */
  size?: "sm" | "md" | "lg";
  /** Enter triggers the confirm emit (leave off for forms that own Enter) */
  enterConfirms?: boolean;
  /** clicking the backdrop cancels (off for must-decide dialogs) */
  backdropCancels?: boolean;
  labelledBy?: string;
}>(), { size: "sm", enterConfirms: false, backdropCancels: true });

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const card = ref<HTMLElement | null>(null);
let lastFocused: HTMLElement | null = null;

const widths: Record<string, string> = { sm: "w-[400px]", md: "w-[520px]", lg: "w-[680px]" };

function focusables(): HTMLElement[] {
  if (!card.value) return [];
  return Array.from(
    card.value.querySelectorAll<HTMLElement>(
      'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])',
    ),
  ).filter((el) => el.offsetParent !== null);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") { e.preventDefault(); emit("cancel"); return; }
  if (e.key === "Enter" && props.enterConfirms) {
    const t = e.target as HTMLElement;
    // don't hijack Enter inside a multiline field
    if (t.tagName !== "TEXTAREA") { e.preventDefault(); emit("confirm"); }
    return;
  }
  if (e.key === "Tab") {
    const els = focusables();
    if (!els.length) return;
    const first = els[0], last = els[els.length - 1];
    const active = document.activeElement as HTMLElement;
    if (e.shiftKey && active === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && active === last) { e.preventDefault(); first.focus(); }
  }
}

onMounted(() => {
  lastFocused = document.activeElement as HTMLElement;
  // focus the element marked data-autofocus, else the first focusable
  const target = card.value?.querySelector<HTMLElement>("[data-autofocus]") ?? focusables()[0];
  target?.focus();
});
onBeforeUnmount(() => lastFocused?.focus?.());
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
    @keydown="onKeydown" @click.self="backdropCancels && emit('cancel')">
    <div ref="card" role="dialog" aria-modal="true" :aria-labelledby="labelledBy"
      class="rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl max-w-[92vw]"
      :class="widths[size]">
      <slot />
    </div>
  </div>
</template>
