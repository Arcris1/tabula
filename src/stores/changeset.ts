import { defineStore } from "pinia";
import { createChangeset } from "../lib/changesetCore";

/** The Data tab's changeset instance; query tabs create their own via createChangeset(). */
export const useChangesetStore = defineStore("changeset", () => createChangeset());
