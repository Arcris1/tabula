<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type ConnectionConfig, type Engine, type EnvColor } from "../lib/api";
import { useConnectionsStore } from "../stores/connections";

const props = defineProps<{ initial?: ConnectionConfig | null }>();
const emit = defineEmits<{ saved: []; cancel: [] }>();
const store = useConnectionsStore();

const defaultPorts: Record<Engine, number | null> = { mysql: 3306, postgres: 5432, sqlite: null, redis: 6379, mssql: 1433 };
const colors: EnvColor[] = ["gray", "blue", "green", "amber", "red"];

const form = reactive<ConnectionConfig>(
  props.initial ? { ...props.initial } : {
    id: crypto.randomUUID(), name: "", engine: "mysql", color: "gray",
    host: "127.0.0.1", port: 3306, username: "", database: "", isProduction: false, ssh: null,
  },
);
const password = ref("");
const sshEnabled = ref(!!form.ssh);
const sshSecret = ref("");
const ssh = reactive(
  form.ssh ? { ...form.ssh } : { host: "", port: 22, username: "", keyPath: "" },
);

function effectiveConfig(): ConnectionConfig {
  return {
    ...form,
    ssh: sshEnabled.value && form.engine !== "sqlite"
      ? { host: ssh.host, port: ssh.port, username: ssh.username, keyPath: ssh.keyPath || null }
      : null,
  };
}
const testState = ref<"idle" | "testing" | "ok" | "fail">("idle");
const testMessage = ref("");

function onEngineChange() {
  form.port = defaultPorts[form.engine];
}
function onColorPick(c: EnvColor) {
  form.color = c;
  if (c === "red") form.isProduction = true;
}
async function test() {
  if (validationError.value) { testState.value = "fail"; testMessage.value = validationError.value; return; }
  testState.value = "testing";
  try {
    await store.test(effectiveConfig(), password.value || undefined, sshSecret.value || undefined);
    testState.value = "ok";
    testMessage.value = "Connection OK";
  } catch (e: any) {
    testState.value = "fail";
    testMessage.value = e?.message ?? String(e);
  }
}
const isFile = () => form.engine === "sqlite";

// Pick the SQLite file through the system dialog. Under the Mac App Store sandbox
// this is the ONLY way the app gets access to it — a typed path grants nothing —
// and remember_file persists that grant across relaunches. Harmless elsewhere.
async function browseSqlite() {
  const path = await openDialog({
    multiple: false,
    filters: [{ name: "SQLite database", extensions: ["sqlite", "sqlite3", "db", "db3"] }],
  });
  if (typeof path === "string") {
    form.database = path;
    try { await api.rememberFile(path); } catch { /* non-sandbox build: no-op */ }
  }
}

// Validate before we bother the network, so a blank host doesn't turn into
// DNS gibberish from the driver. Server engines need a host + a valid port;
// SQLite needs a file path.
const validationError = computed<string | null>(() => {
  if (!(form.name ?? "").trim()) return "Give the connection a name.";
  if (isFile()) return (form.database ?? "").trim() ? null : "Choose a SQLite database file.";
  if (!(form.host ?? "").trim()) return "Host is required.";
  if (!form.port || form.port < 1 || form.port > 65535) return "Port must be between 1 and 65535.";
  if (sshEnabled.value && !(ssh.host ?? "").trim()) return "SSH host is required when using a tunnel.";
  return null;
});

async function save() {
  if (validationError.value) return;
  await store.save(effectiveConfig(), password.value || undefined, sshSecret.value || undefined);
  emit("saved");
}
</script>

<template>
  <form class="flex flex-col gap-3 w-96" @submit.prevent="save">
    <input v-model="form.name" placeholder="Connection name" required
      class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5 focus:border-zinc-600 outline-none" />

    <select v-model="form.engine" @change="onEngineChange"
      class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5">
      <option value="mysql">MySQL / MariaDB</option>
      <option value="postgres">PostgreSQL</option>
      <option value="mssql">SQL Server</option>
      <option value="sqlite">SQLite</option>
      <option value="redis">Redis</option>
    </select>

    <div class="flex gap-2 items-center">
      <span class="text-zinc-500 text-xs">Environment</span>
      <button v-for="c in colors" :key="c" type="button" @click="onColorPick(c)"
        class="w-4 h-4 rounded-full border"
        :class="[
          { gray: 'bg-zinc-500', blue: 'bg-blue-500', green: 'bg-emerald-500', amber: 'bg-amber-500', red: 'bg-red-500' }[c],
          form.color === c ? 'border-white' : 'border-transparent opacity-50',
        ]" />
      <label class="ml-auto flex items-center gap-1 text-xs text-zinc-400">
        <input type="checkbox" v-model="form.isProduction" /> production
      </label>
    </div>

    <template v-if="!isFile()">
      <div class="flex gap-2">
        <input v-model="form.host" placeholder="Host" class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
        <input v-model.number="form.port" type="number" placeholder="Port" class="w-24 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      </div>
      <input v-if="form.engine !== 'redis'" v-model="form.username" placeholder="Username"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      <input v-model="password" type="password" placeholder="Password (stored in Keychain)"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      <input v-model="form.database" :placeholder="form.engine === 'redis' ? 'DB index (0)' : 'Database'"
        class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
    </template>
    <div v-else class="flex gap-2">
      <input v-model="form.database" placeholder="/path/to/database.sqlite"
        class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      <button type="button" @click="browseSqlite"
        class="px-3 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800 text-sm">Browse…</button>
    </div>

    <template v-if="!isFile()">
      <label class="flex items-center gap-2 text-xs text-zinc-400 pt-1">
        <input type="checkbox" v-model="sshEnabled" /> Connect through SSH tunnel
      </label>
      <template v-if="sshEnabled">
        <div class="flex gap-2">
          <input v-model="ssh.host" placeholder="SSH host" class="flex-1 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
          <input v-model.number="ssh.port" type="number" placeholder="22" class="w-20 bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
        </div>
        <input v-model="ssh.username" placeholder="SSH user" class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
        <input v-model="ssh.keyPath" placeholder="Private key path (empty = password auth)"
          class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
        <input v-model="sshSecret" type="password"
          :placeholder="ssh.keyPath ? 'Key passphrase (Keychain)' : 'SSH password (Keychain)'"
          class="bg-zinc-900 border border-zinc-800 rounded px-2 py-1.5" />
      </template>
    </template>

    <div class="flex gap-2 items-center pt-1">
      <button type="button" @click="test" class="px-3 py-1.5 rounded border border-zinc-700 hover:bg-zinc-800">
        {{ testState === "testing" ? "Testing…" : "Test Connection" }}
      </button>
      <span v-if="testState === 'ok'" class="text-emerald-400 text-xs">{{ testMessage }}</span>
      <span v-if="testState === 'fail'" class="text-red-400 text-xs truncate">{{ testMessage }}</span>
      <div class="ml-auto flex gap-2">
        <button type="button" @click="emit('cancel')" class="px-3 py-1.5 rounded text-zinc-400 hover:bg-zinc-800">Cancel</button>
        <button type="submit" :disabled="!!validationError" :title="validationError ?? ''"
          class="px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white disabled:opacity-40 disabled:cursor-not-allowed">Save</button>
      </div>
    </div>
    <p v-if="validationError" class="text-amber-400/80 text-xs -mt-1">{{ validationError }}</p>
  </form>
</template>
