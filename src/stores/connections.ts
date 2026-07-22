import { defineStore } from "pinia";
import { ref } from "vue";
import { api, type ConnectionConfig } from "../lib/api";

export const useConnectionsStore = defineStore("connections", () => {
  const connections = ref<ConnectionConfig[]>([]);

  async function load() {
    connections.value = await api.listConnections();
  }
  async function save(cfg: ConnectionConfig, password?: string, sshSecret?: string) {
    await api.saveConnection(cfg, password, sshSecret);
    await load();
  }
  async function remove(id: string) {
    await api.deleteConnection(id);
    await load();
  }
  async function test(cfg: ConnectionConfig, password?: string, sshSecret?: string) {
    await api.testConnection(cfg, password, sshSecret);
  }
  async function connect(id: string, database?: string) {
    return api.connect(id, database);
  }
  async function disconnect(id: string) {
    await api.disconnect(id);
  }

  return { connections, load, save, remove, test, connect, disconnect };
});
