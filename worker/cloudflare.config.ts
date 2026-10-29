import { bindings, defineConfig } from "cf/config";
import * as entrypoint from "./src/index.ts" with { type: "cf-worker" };

export default defineConfig({
	worker: {
		name: "ac-scheduler-cloud",
		compatibilityDate: "2026-09-25",
		entrypoint,
		env: {
			// Last home/away state plus a short history, written by the phone geofence
			PRESENCE: bindings.kv(),
			// Shared bearer token the iPhone Shortcuts send in the Authorization header
			PRESENCE_TOKEN: bindings.secret(),
		},
	},
});
