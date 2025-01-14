import { SELECTED_PROVIDER_KEY } from '../components/setup/ApiKeySetupCard';

export const getCurrentProvider = (): string => {
  const provider = localStorage.getItem(SELECTED_PROVIDER_KEY);
  console.log('Getting current provider:', provider || 'none');
  return provider || 'openai'; // default to OpenAI if none selected
};

export const getProviderKeyName = (providerId: string): string => {
  return providerId === 'openai' ? 'OPENAI_API_KEY' : 'ANTHROPIC_API_KEY';
}; 