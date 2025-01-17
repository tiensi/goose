import { getApiUrl  } from "../config";

export const SELECTED_PROVIDER_KEY = "GOOSE_PROVIDER__API_KEY"

export interface ProviderOption {
  id: string;
  name: string;
  description: string;
  models: string;
}

export const OPENAI_ENDPOINT_PLACEHOLDER = "https://api.openai.com";
export const ANTHROPIC_ENDPOINT_PLACEHOLDER = "https://api.anthropic.com";
export const OPENAI_DEFAULT_MODEL = "gpt-4"
export const ANTHROPIC_DEFAULT_MODEL = "claude-3-sonnet"

// TODO we will provide these from a rust endpoint
export const providers: ProviderOption[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    description: 'Use GPT-4 and other OpenAI models',
    modelExample: 'gpt-4-turbo'
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    description: 'Use Claude and other Anthropic models',
    modelExample: 'claude-3-sonnet'
  }
];

export function getStoredProvider(config: any): string | null {
  return config.GOOSE_PROVIDER || localStorage.getItem("GOOSE_PROVIDER");
}

export interface Provider {
  id: string; // Lowercase key (e.g., "openai")
  name: string; // Provider name (e.g., "OpenAI")
  description: string; // Description of the provider
  models: string[]; // List of supported models
  requiredKeys: string[]; // List of required keys
}

export async function getProvidersList(): Promise<Provider[]> {
  const response = await fetch(getApiUrl("/agent/providers"), {
    method: "GET",
  });

  if (!response.ok) {
    throw new Error(`Failed to fetch providers: ${response.statusText}`);
  }

  const data = await response.json();
  console.log("Raw API Response:", data); // Log the raw response


  // Format the response into an array of providers
  return data.map((item: any) => ({
    id: item.id, // Root-level ID
    name: item.details?.name || "Unknown Provider", // Nested name in details
    description: item.details?.description || "No description available.", // Nested description
    models: item.details?.models || [], // Nested models array
    requiredKeys: item.details?.required_keys || [], // Nested required keys array
  }));
}



