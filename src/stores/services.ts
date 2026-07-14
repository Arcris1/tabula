import { defineStore } from "pinia";
import { ref } from "vue";
import { api, type ServiceInfo } from "../lib/api";

export type ServiceAction = "start" | "stop" | "restart";

/** Local dev services (databases, later web stack) managed via the OS. */
export const useServicesStore = defineStore("services", () => {
  const services = ref<ServiceInfo[]>([]);
  const loading = ref(false);
  const busy = ref<Set<string>>(new Set()); // ids with an action in flight
  const error = ref<string | null>(null);

  async function load() {
    loading.value = true;
    try {
      services.value = await api.listServices();
      error.value = null;
    } catch (e: any) {
      error.value = e?.message ?? String(e);
    } finally {
      loading.value = false;
    }
  }

  async function act(id: string, action: ServiceAction) {
    busy.value = new Set(busy.value).add(id);
    error.value = null;
    try {
      await api.serviceAction(id, action);
    } catch (e: any) {
      error.value = `${action} ${id}: ${e?.message ?? String(e)}`;
    } finally {
      const b = new Set(busy.value);
      b.delete(id);
      busy.value = b;
      await load(); // reflect the new status
    }
  }

  async function startAll() {
    for (const s of services.value.filter((s) => s.manageable && !s.running)) {
      await act(s.id, "start");
    }
  }
  async function stopAll() {
    for (const s of services.value.filter((s) => s.manageable && s.running)) {
      await act(s.id, "stop");
    }
  }

  return { services, loading, busy, error, load, act, startAll, stopAll };
});
