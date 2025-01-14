import React from 'react';
import { ApiKeySetupCard } from './ApiKeySetupCard';


interface NewWelcomeScreenProps {
  className?: string;
  onDismiss: () => void;
}

export function NewWelcomeScreen({ className, onDismiss }: NewWelcomeScreenProps) {
  const handleApiKeySubmit = (provider: string, apiKey: string) => {
    console.log('Would configure provider:', provider);
    console.log('Would send API key to backend endpoint');
    console.log('For now, just dismissing...');
    onDismiss();
  };

  return (
    <ApiKeySetupCard 
      className={className} 
      onSubmit={handleApiKeySubmit}
    />
  );
} 