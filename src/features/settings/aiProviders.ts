import type { AiProvider } from "@/api";

export interface AiProviderInfo {
  id: AiProvider;
  /** In the provider menu. */
  label: string;
  /** The company, for "…'s servers". */
  company: string;
  /** The computer the summary is sent to ("api.openai.com"); null for a custom address or this computer. */
  host: string | null;
  /** The name of the key, for the field label: "OpenAI API key". */
  keyLabel: string | null;
  keyPlaceholder: string;
  /** Where to create an API key. https only. */
  keyUrl: string | null;
  defaultModel: string;
}

/** Keep the ids, hosts and default models in step with `AiProvider` in firstlight-core. */
export const AI_PROVIDERS: AiProviderInfo[] = [
  { id: "anthropic", company: "Anthropic", label: "Claude (Anthropic)", host: "api.anthropic.com", keyLabel: "Anthropic API key", keyPlaceholder: "sk-ant-…", keyUrl: "https://console.anthropic.com/settings/keys", defaultModel: "claude-haiku-5-5" },
  { id: "openai", company: "OpenAI", label: "ChatGPT models (OpenAI)", host: "api.openai.com", keyLabel: "OpenAI API key", keyPlaceholder: "sk-…", keyUrl: "https://platform.openai.com/api-keys", defaultModel: "gpt-4o-mini" },
  { id: "xai", company: "xAI", label: "Grok (xAI)", host: "api.x.ai", keyLabel: "xAI API key", keyPlaceholder: "xai-…", keyUrl: "https://console.x.ai", defaultModel: "grok-3-mini" },
  { id: "gemini", company: "Google", label: "Google Gemini", host: "generativelanguage.googleapis.com", keyLabel: "Gemini API key", keyPlaceholder: "AIza…", keyUrl: "https://aistudio.google.com/apikey", defaultModel: "gemini-2.0-flash" },
  { id: "mistral", company: "Mistral", label: "Mistral", host: "api.mistral.ai", keyLabel: "Mistral API key", keyPlaceholder: "", keyUrl: "https://console.mistral.ai/api-keys", defaultModel: "mistral-small-latest" },
  { id: "groq", company: "Groq", label: "Groq", host: "api.groq.com", keyLabel: "Groq API key", keyPlaceholder: "gsk_…", keyUrl: "https://console.groq.com/keys", defaultModel: "llama-3.3-70b-versatile" },
  { id: "openrouter", company: "OpenRouter", label: "OpenRouter (many models)", host: "openrouter.ai", keyLabel: "OpenRouter API key", keyPlaceholder: "sk-or-…", keyUrl: "https://openrouter.ai/keys", defaultModel: "openai/gpt-4o-mini" },
  { id: "custom", company: "The service", label: "Another service (OpenAI-compatible)", host: null, keyLabel: "API key", keyPlaceholder: "", keyUrl: null, defaultModel: "gpt-4o-mini" },
  { id: "local", company: "This computer", label: "A model on this computer", host: null, keyLabel: null, keyPlaceholder: "", keyUrl: null, defaultModel: "llama3.2" },
];

export const aiProvider = (id: AiProvider): AiProviderInfo => AI_PROVIDERS.find((p) => p.id === id) ?? AI_PROVIDERS[0]!;

/** The host of an https address, or null if it is not one. */
export function hostOf(address: string): string | null {
  try {
    const u = new URL(address.trim());
    return u.protocol === "https:" ? u.hostname : null;
  } catch {
    return null;
  }
}
