import React, { useEffect, useState } from 'react';
import { getApiUrl, getSecretKey } from "../../config";
import { FaKey, FaExclamationCircle, FaPencilAlt, FaTrash } from 'react-icons/fa';
import { showToast } from '../ui/toast';

interface SecretSource {
  key: string;
  source: string;
  is_set: boolean;
}

interface Provider {
  id: string;
  name: string;
  keys: string[];
  description: string;
  canDelete?: boolean;
}

interface ProviderSecretStatus {
  is_set: boolean;
  location: string | null;
}

interface ProviderResponse {
  supported: boolean;
  secret_status?: Record<string, ProviderSecretStatus>;
}

const PROVIDERS: Provider[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    keys: ['OPENAI_API_KEY'],
    description: 'OpenAI API access for GPT models'
  },
  {
    id: 'anthropic',
    name: 'Anthropic',
    keys: ['ANTHROPIC_API_KEY'],
    description: 'Anthropic API access for Claude models'
  },
  {
    id: 'databricks',
    name: 'Databricks',
    keys: ['DATABRICKS_API_KEY'],
    description: 'Databricks API access',
    canDelete: true
  },
  {
    id: 'otherProvider',
    name: 'OtherProvider',
    keys: ['MY_API_KEY'],
    description: 'OtherProvider API access',
    canDelete: true
  },


];

// Mock data that matches the Rust endpoint response format
const MOCK_SECRETS_RESPONSE = {
  'openai': {
    supported: true,
    secret_status: {
      'OPENAI_API_KEY': {
        is_set: true,
        location: 'keyring'
      }
    }
  },
  'anthropic': {
    supported: true,
    secret_status: {
      'ANTHROPIC_API_KEY': {
        is_set: false,
        location: null
      }
    }
  },
  'databricks': {
    supported: true,
    secret_status: {
      'DATABRICKS_API_KEY': {
        is_set: true,
        location: 'env'
      }
    }
  }, 
  'otherProvider': {
    supported: false,
  }
};

export default function Keys() {
  const [secrets, setSecrets] = useState<SecretSource[]>([]);
  const [expandedProviders, setExpandedProviders] = useState<Set<string>>(new Set());
  const [providers, setProviders] = useState<Provider[]>(PROVIDERS);

  useEffect(() => {
    const fetchSecrets = async () => {
      try {
        await new Promise(resolve => setTimeout(resolve, 500));
        
        // Transform only supported providers with secret_status
        const transformedSecrets = Object.entries(MOCK_SECRETS_RESPONSE)
          .filter(([_, status]) => status.supported && status.secret_status)
          .flatMap(([_, status]) => 
            Object.entries(status.secret_status!).map(([key, secretStatus]) => ({
              key,
              source: secretStatus.location || 'none',
              is_set: secretStatus.is_set
            }))
          );
        
        setSecrets(transformedSecrets);
      } catch (error) {
        console.error('Error fetching secrets:', error);
      }
    };

    fetchSecrets();
  }, []);

  const getProviderStatus = (provider: Provider) => {
    const providerSecrets = provider.keys.map(key => 
      secrets.find(s => s.key === key)
    );
    return providerSecrets.some(s => !s?.is_set);
  };

  const handleEdit = (key: string) => {
    showToast("Key edited and updated in the keychain", "success");
  };

  const handleDeleteKey = (providerId: string, key: string) => {
    showToast(`Key ${key} deleted`, "success");
  };

  const handleDeleteProvider = (providerId: string) => {
    setProviders(providers.filter(p => p.id !== providerId));
    showToast(`Provider ${providerId} removed`, "success");
  };

  const toggleProvider = (providerId: string) => {
    setExpandedProviders(prev => {
      const newSet = new Set(prev);
      if (newSet.has(providerId)) {
        newSet.delete(providerId);
      } else {
        newSet.add(providerId);
      }
      return newSet;
    });
  };

  const isProviderSupported = (providerId: string) => {
    return MOCK_SECRETS_RESPONSE[providerId]?.supported ?? false;
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-2xl font-semibold dark:text-white">Providers</h1>
        <button className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600">
          Add Provider
        </button>
      </div>

      <div className="grid gap-4">
        {providers.map((provider) => {
          const hasUnsetKeys = getProviderStatus(provider);
          const isExpanded = expandedProviders.has(provider.id);
          const isSupported = isProviderSupported(provider.id);
          
          return (
            <div key={provider.id} className="border dark:border-gray-700 rounded-lg p-4">
              <div className="flex items-center justify-between">
                <button 
                  className="flex-1 flex items-center justify-between"
                  onClick={() => isSupported && toggleProvider(provider.id)}
                >
                  <div className="flex items-center gap-3">
                    <div className="w-8 h-8 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center">
                      <FaKey className={`${isSupported ? 'text-gray-500' : 'text-red-500'}`} />
                    </div>
                    <div className="text-left">
                      <h3 className="font-medium dark:text-white">{provider.name}</h3>
                      <p className="text-sm text-gray-500">
                        {isSupported ? provider.description : 'Provider not supported'}
                      </p>
                    </div>
                  </div>
                  {isSupported && hasUnsetKeys && (
                    <FaExclamationCircle className="text-yellow-500" />
                  )}
                </button>
                {provider.canDelete && (
                  <button
                    onClick={() => handleDeleteProvider(provider.id)}
                    className="ml-4 p-1.5 text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-200 rounded-full hover:bg-red-100 dark:hover:bg-red-900"
                    title="Delete provider"
                  >
                    <FaTrash size={14} />
                  </button>
                )}
              </div>

              {isSupported && isExpanded && (
                <div className="mt-4 pl-11">
                  {provider.keys.map(key => {
                    const secret = secrets.find(s => s.key === key);
                    return (
                      <div key={key} className="py-2 flex items-center justify-between">
                        <div>
                          <p className="text-sm font-mono dark:text-gray-300">{key}</p>
                          <p className="text-xs text-gray-500">
                            Source: {secret?.source || 'none'}
                          </p>
                        </div>
                        <div className="flex items-center gap-2">
                          <span className={`px-2 py-1 rounded text-xs ${
                            secret?.is_set 
                              ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' 
                              : 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200'
                          }`}>
                            {secret?.is_set ? 'Set' : 'Not Set'}
                          </span>
                          <button
                            onClick={() => handleEdit(key)}
                            className="p-1.5 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 rounded-full hover:bg-gray-100 dark:hover:bg-gray-700"
                            title="Edit key"
                          >
                            <FaPencilAlt size={14} />
                          </button>
                          <button
                            onClick={() => handleDeleteKey(provider.id, key)}
                            className="p-1.5 text-red-500 hover:text-red-700 dark:text-red-400 dark:hover:text-red-200 rounded-full hover:bg-red-100 dark:hover:bg-red-900"
                            title="Delete key"
                          >
                            <FaTrash size={14} />
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
} 