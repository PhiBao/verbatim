// Traces every OpenCode turn in this repository to neatlogs.
//
// This is deliberate: the agent that built the product is itself observed by the
// product's sponsor, so the build log is evidence rather than a claim.
//
// OpenCode v2 requires the default export to be a definition object with an `id`
// and an `effect` function — a bare function fails validation with "must export a
// default definition with an id and an effect or setup function". The neatlogs
// package exports the plugin as a function, so it is wrapped here.
import { NeatlogsOpencodePlugin } from "neatlogs/opencode";

export default {
  id: "neatlogs",
  effect: NeatlogsOpencodePlugin,
};
