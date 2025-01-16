import React, { useEffect, useState } from 'react';
import { getApiUrl, getSecretKey } from "../../config";
import { FaKey, FaExclamationCircle, FaPencilAlt, FaTrash, FaArrowLeft } from 'react-icons/fa';
import { showToast } from '../ui/toast';
import { useNavigate } from 'react-router-dom';
import { 
  Modal, 
  ModalContent, 
  ModalHeader, 
  ModalTitle 
} from '../ui/modal';

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

interface ProviderStatusResponse {
  [provider: string]: {
    set: boolean;
    location: string | null;
    supported: boolean;
  };
}

export default function Keys() {
  const navigate = useNavigate();
  const [secrets, setSecrets] = useState<SecretSource[]>([]);
  const [expandedProviders, setExpandedProviders] = useState<Set<string>>(new Set());
  const [providers, setProviders] = useState<Provider[]>(PROVIDERS);
  const [showTestModal, setShowTestModal] = useState(false);
  const [testResponse, setTestResponse] = useState<ProviderStatusResponse | null>(null);

  useEffect(() => {
    const fetchSecrets = async () => {
      try {
        const response = await fetch(getApiUrl("/secrets/providers"), {
            method: 'POST',
            headers: {
              'Content-Type': 'application/json',
              'X-Secret-Key': getSecretKey(),
            },
            body: JSON.stringify({
              providers: ["OpenAI", "Anthropic", "MyProvider"]
            })
        });

        if (!response.ok) {
          throw new Error('Failed to fetch secrets');
        }

        let data = await response.json();
        console.log(data)
        
        // Append test data for additional providers
        data = {
          ...data,
          'databricks': {
            supported: true,
            secret_status: {
              'DATABRICKS_API_KEY': {
                is_set: false,
                location: null
              }
            }
          }
        };

        // Transform only supported providers with secret_status
        const transformedSecrets = Object.entries(data)
          .filter(([_, status]) => status.supported && status.secret_status)
          .flatMap(([_, status]) => 
            Object.entries(status.secret_status!).map(([key, secretStatus]) => ({
              key,
              source: secretStatus.location || 'none',
              is_set: secretStatus.is_set
            }))
          );
        
        setSecrets(transformedSecrets);
        
        // Check the GOOSE_PROVIDER from localStorage
        const gooseProvider = localStorage.getItem("GOOSE_PROVIDER")?.toLowerCase() || null;
        if (gooseProvider) {
          const matchedProvider = PROVIDERS.find(provider => provider.id.toLowerCase() === gooseProvider);
          if (matchedProvider) {
            setExpandedProviders(new Set([matchedProvider.id]));
          } else {
            console.warn(`Provider ${gooseProvider} not found in settings.`);
          }
        }
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

  const handleDeleteKey = async (providerId: string, key: string) => {
    // Find the secret to check its source
    const secret = secrets.find(s => s.key === key);
    
    if (secret?.source === 'env') {
      showToast("This key is set in your environment. Please remove it from your ~/.zshrc or equivalent file.", "error");
      return;
    }

    try {
      const response = await fetch(getApiUrl("/secrets/delete"), {
        method: 'DELETE',
        headers: {
          'Content-Type': 'application/json',
          'X-Secret-Key': getSecretKey(),
        },
        body: JSON.stringify({ key })
      });

      if (!response.ok) {
        throw new Error('Failed to delete key');
      }

      // Update local state to reflect deletion
      setSecrets(secrets.filter(s => s.key !== key));
      showToast(`Key ${key} deleted from keychain`, "success");
    } catch (error) {
      console.error('Error deleting key:', error);
      showToast("Failed to delete key", "error");
    }
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

  const handleTestProviders = async () => {
    try {
      const response = await fetch(getApiUrl("/secrets/providers"), {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-Secret-Key': getSecretKey(),
        },
        body: JSON.stringify({
          providers: ["OpenAI", "Anthropic", "MyProvider"]
        })
      });

      if (!response.ok) {
        throw new Error('Failed to fetch provider status');
      }

      const data = await response.json();
      setTestResponse(data);
      setShowTestModal(true);
    } catch (error) {
      console.error('Error testing providers:', error);
      showToast("Failed to test providers", "error");
    }
  };

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <div className="flex items-center mb-6">
        <button 
          onClick={() => navigate(-1)}
          className="mr-4 p-2 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-full transition-colors"
          title="Go back"
        >
          <FaArrowLeft className="text-gray-500 dark:text-gray-400" />
        </button>
        <h1 className="text-2xl font-semibold dark:text-white flex-1">Providers</h1>
        <div className="flex gap-2">
          <button 
            onClick={handleTestProviders}
            className="px-4 py-2 bg-gray-500 text-white rounded-lg hover:bg-gray-600"
          >
            Test Providers
          </button>
          <button className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600">
            Add Provider
          </button>
        </div>
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
                      <div className="flex items-center gap-2">
                        <h3 className="font-medium dark:text-white">{provider.name}</h3>
                        {provider.id.toLowerCase() === (localStorage.getItem("GOOSE_PROVIDER")?.toLowerCase() || '') && (
                          <span className="text-xs px-2 py-0.5 bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200 rounded-full">
                            Selected Provider
                          </span>
                        )}
                        {!isSupported && (
                          <span className="text-xs px-2 py-0.5 bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200 rounded-full">
                            Not Supported
                          </span>
                        )}
                      </div>
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
                            {secret?.is_set ? 'Key set' : 'Missing'}
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

      <Modal open={showTestModal} onOpenChange={setShowTestModal}>
        <ModalContent>
          <ModalHeader>
            <ModalTitle>Provider Status Test</ModalTitle>
          </ModalHeader>
          <div className="mt-4">
            <pre className="bg-gray-100 dark:bg-gray-900 p-4 rounded-lg overflow-auto max-h-96 text-sm">
              {testResponse && JSON.stringify(testResponse, null, 2)}
            </pre>
          </div>
        </ModalContent>
      </Modal>
    </div>
  );
} 