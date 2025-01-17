import { ProviderOption } from './providerUtils';
import { getApiUrl, getSecretKey, addMCP, addMCPSystem } from '../config';

export const addAgent = async (provider: ProviderOption) => {
  const response = await fetch(getApiUrl("/agent"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Secret-Key": getSecretKey(),
    },
    body: JSON.stringify({ provider: provider.id }),
  });

  if (!response.ok) {
    throw new Error(`Failed to add agent: ${response.statusText}`);
  }

  return response;
};

export const addSystemConfig = async (system: string) => {
  const systemConfig = {
    type: "Stdio",
    cmd: await window.electron.getBinaryPath("goosed"),
    args: ["mcp", system],
  };

  const response = await fetch(getApiUrl("/systems/add"), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Secret-Key": getSecretKey(),
    },
    body: JSON.stringify(systemConfig),
  });

  if (!response.ok) {
    throw new Error(`Failed to add system config for ${system}: ${response.statusText}`);
  }

  return response;
};

export const initializeSystem = async (provider: ProviderOption) => {
  try {
    await addAgent(provider);
    await addSystemConfig("developer2");

    // Handle deep link if present
    const deepLink = window.appConfig.get('DEEP_LINK');
    if (deepLink) {
      await addMCPSystem(deepLink);
    }
  } catch (error) {
    console.error("Failed to initialize system:", error);
    throw error;
  }
}; 