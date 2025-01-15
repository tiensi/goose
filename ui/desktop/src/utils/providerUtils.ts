export const SELECTED_PROVIDER_KEY = "GOOSE_PROVIDER__API_KEY"

export interface ProviderOption {
  id: 'openai' | 'anthropic';
  name: string;
  logo: string;
  description: string;
  modelExample: string;
}

export const providers: ProviderOption[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    logo: '🤖',
    description: 'Use GPT-4 and other OpenAI models',
    modelExample: 'gpt-4-turbo'
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    logo: '🧠',
    description: 'Use Claude and other Anthropic models',
    modelExample: 'claude-3-sonnet'
  }
];

export const getCurrentProvider = (): string => {
  const provider = localStorage.getItem(SELECTED_PROVIDER_KEY);
  console.log('Getting current provider:', provider || 'none');
  return provider || 'openai'; // default to OpenAI if none selected
};

export const getProviderKeyName = (providerId: string): string => {
  return providerId === 'openai' ? 'OPENAI_API_KEY' : 'ANTHROPIC_API_KEY';
}; 