import React, { useState } from 'react';
import { Card } from '../ui/card';
import { Bird } from '../ui/icons';
import { GOOSE_WELCOME_MESSAGE, GOOSE_WELCOME_MESSAGE_HEADER } from './constants';
import { WelcomeModelModal } from './WelcomePageModelModal';
import { providers, ProviderOption } from '../../utils/providerUtils';
import { OPENAI_ENDPOINT_PLACEHOLDER, ANTHROPIC_ENDPOINT_PLACEHOLDER, OPENAI_DEFAULT_MODEL, ANTHROPIC_DEFAULT_MODEL } from './constants';
import mockKeychain from '../../services/mockKeychain';
import { PROVIDER_API_KEY } from '../../ChatWindow';

interface NewWelcomeScreenProps {
  onDismiss: () => void;
  className?: string;
}

export function NewWelcomeScreen({ className = '' }: NewWelcomeScreenProps) {
  const [showModal, setShowModal] = useState(false);
  const [selectedProvider, setSelectedProvider] = useState<ProviderOption | null>(null);

  const handleProviderSelect = (provider: ProviderOption) => {
    setSelectedProvider(provider);
    setShowModal(true);
  };

  const handleModalSubmit = async (apiKey: string, provider: string) => {
    try {
      const trimmedKey = apiKey.trim();
      await mockKeychain.setKey(PROVIDER_API_KEY, trimmedKey);
      setShowModal(false);
      console.log("Setting GOOSE_PROVIDER to:", selectedProvider.name);
      localStorage.setItem("GOOSE_PROVIDER", selectedProvider.name);
      console.log("Set GOOSE_PROVIDER: ", localStorage.getItem("GOOSE_PROVIDER"));
      window.electron.createChatWindow();  
    } catch (error) {
      console.error('Failed to store API key:', error);
    }
  };

  return (
    <>
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
          
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-6">
            {providers.map((provider) => (
              <button
                key={provider.id}
                onClick={() => handleProviderSelect(provider)}
                className="p-6 border rounded-lg hover:border-blue-500 transition-colors text-left dark:border-gray-700 dark:hover:border-blue-400"
              >
                <div className="text-2xl mb-2">{provider.logo}</div>
                <h3 className="text-lg font-medium mb-2 dark:text-gray-200">{provider.name}</h3>
                <p className="text-gray-600 dark:text-gray-400">{provider.description}</p>
              </button>
            ))}
          </div>
        </div>
      </Card>

      {showModal && selectedProvider && (
        <WelcomeModelModal
          provider={selectedProvider.name}
          model={selectedProvider.id === 'openai' ? OPENAI_DEFAULT_MODEL : ANTHROPIC_DEFAULT_MODEL}
          endpoint={selectedProvider.id === 'openai' ? OPENAI_ENDPOINT_PLACEHOLDER : ANTHROPIC_ENDPOINT_PLACEHOLDER}
          onSubmit={handleModalSubmit}
          onCancel={() => setShowModal(false)}
        />
      )}
    </>
  );
}
