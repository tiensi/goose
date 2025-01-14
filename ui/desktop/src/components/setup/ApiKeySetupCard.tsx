import React, { useState } from 'react';
import { Card } from '../ui/card';
import { GOOSE_WELCOME_MESSAGE, GOOSE_WELCOME_MESSAGE_HEADER } from './constants';
import { Bird } from '../ui/icons';
import mockKeychain from '../../services/mockKeychain';
import { PROVIDER_API_KEY } from '../../ChatWindow';

interface ApiKeySetupCardProps {
  onSubmit: (provider: string, apiKey: string) => void;
  className?: string;
}

interface ProviderOption {
  id: 'openai' | 'anthropic';
  name: string;
  logo: string;
  description: string;
  modelExample: string;
}

const providers: ProviderOption[] = [
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

export const OPENAI_API_KEY = "OPENAI_API_KEY";
export const ANTHROPIC_API_KEY = "ANTHROPIC_API_KEY";
export const SELECTED_PROVIDER_KEY = "selected_provider"; // localStorage key for provider preference

export function ApiKeySetupCard({ onSubmit, className }: ApiKeySetupCardProps) {
  const [selectedProvider, setSelectedProvider] = useState<ProviderOption | null>(() => {
    // Initialize with saved provider preference
    const savedProvider = localStorage.getItem(SELECTED_PROVIDER_KEY);
    console.log('Loading saved provider preference:', savedProvider);
    return providers.find(p => p.id === savedProvider) || null;
  });
  const [apiKey, setApiKey] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    console.log('Attempting API key submission:', {
      provider: selectedProvider?.id,
      hasKey: !!apiKey.trim(),
      isSubmitting
    });

    if (!selectedProvider || !apiKey.trim()) {
      console.warn('Submission blocked:', {
        hasProvider: !!selectedProvider,
        hasKey: !!apiKey.trim()
      });
      return;
    }

    setIsSubmitting(true);
    try {
      const trimmedKey = apiKey.trim();
      
      // Save the provider-specific API key
      const providerKeyName = selectedProvider.id === 'openai' ? OPENAI_API_KEY : ANTHROPIC_API_KEY;
      await mockKeychain.setKey(providerKeyName, trimmedKey);
      
      // Save the generic provider key
      await mockKeychain.setKey(PROVIDER_API_KEY, trimmedKey);
      
      // Save the selected provider preference
      localStorage.setItem(SELECTED_PROVIDER_KEY, selectedProvider.id);
      
      console.log('Successfully stored keys for provider:', {
        provider: selectedProvider.id,
        genericKey: PROVIDER_API_KEY,
        providerKey: providerKeyName
      });
      
      onSubmit(selectedProvider.id, trimmedKey);
    } catch (error) {
      console.error('Failed to store API key:', {
        provider: selectedProvider.id,
        error: error instanceof Error ? error.message : String(error)
      });
    } finally {
      console.log('Submission process completed:', {
        provider: selectedProvider.id,
        success: !isSubmitting
      });
      setIsSubmitting(false);
    }
  };

  // Add handler for provider selection
  const handleProviderChange = (provider: ProviderOption) => {
    setSelectedProvider(provider);
    console.log('Provider selection changed:', provider.id);
  };

  return (
    <Card className={`flex flex-col items-center p-8 space-y-6 bg-card-gradient dark:bg-dark-card-gradient w-full h-full ${className}`}>
      <div className="w-16 h-16">
        <Bird />
      </div>
      
      <div className="text-center space-y-6 max-w-2xl w-full">
        <h2 className="text-2xl font-semibold text-gray-800 dark:text-gray-200">
          {GOOSE_WELCOME_MESSAGE_HEADER}
        </h2>
        
        <p className="text-gray-600 dark:text-white/50">
          {GOOSE_WELCOME_MESSAGE}
        </p>
        
        {!selectedProvider && (
          <p className="text-gray-600 dark:text-gray-400">
            Choose your AI provider to get started
          </p>
        )}
        
        {!selectedProvider ? (
          <>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-6">
              {providers.map((provider) => (
                <button
                  key={provider.id}
                  onClick={() => handleProviderChange(provider)}
                  className="p-6 border rounded-lg hover:border-blue-500 transition-colors text-left dark:border-gray-700 dark:hover:border-blue-400"
                >
                  <div className="text-2xl mb-2">{provider.logo}</div>
                  <h3 className="text-lg font-medium mb-2 dark:text-gray-200">{provider.name}</h3>
                  <p className="text-gray-600 dark:text-gray-400">{provider.description}</p>
                </button>
              ))}
            </div>
          </>
        ) : (
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="flex items-center mb-4">
              <button
                type="button"
                onClick={() => setSelectedProvider(null)}
                className="text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300"
              >
                ← Back
              </button>
              <h3 className="text-xl font-medium ml-4 dark:text-gray-200">
                Enter your {selectedProvider.name} API Key
              </h3>
            </div>

            <div className="space-y-2">
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={`Paste your ${selectedProvider.name} API key here`}
                className="w-full p-2 border rounded-md dark:bg-gray-800 dark:border-gray-700 dark:text-gray-200"
                required
              />
              <p className="text-sm text-gray-500 dark:text-gray-400">
                Example model: {selectedProvider.modelExample}
              </p>
            </div>

            <button
              type="submit"
              disabled={isSubmitting || !apiKey.trim()}
              className={`w-full py-2 px-4 rounded-md text-white transition-colors ${
                isSubmitting || !apiKey.trim()
                  ? 'bg-gray-400 dark:bg-gray-600'
                  : 'bg-blue-500 hover:bg-blue-600 dark:bg-blue-600 dark:hover:bg-blue-700'
              }`}
            >
              {isSubmitting ? 'Setting up...' : 'Continue'}
            </button>

            <p className="text-sm text-gray-600 dark:text-gray-400 mt-4">
              Your API key will be stored securely and used only for making requests to {selectedProvider.name}.
            </p>
          </form>
        )}
      </div>
    </Card>
  );
} 