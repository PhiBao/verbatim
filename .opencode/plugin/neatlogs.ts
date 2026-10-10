// Traces every OpenCode turn in this repository to neatlogs.
//
// This is deliberate: the agent that built the product is itself observed by the
// product's sponsor, so the build log is evidence rather than a claim. Each turn
// becomes a trace grouped by session id.
//
// See https://docs.neatlogs.com/integrations/opencode
export { NeatlogsOpencodePlugin as default } from "neatlogs/opencode";
