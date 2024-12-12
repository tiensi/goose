import React from 'react';
import { featureFlags, type FeatureFlags } from '../featureFlags';

export default function FeatureFlagsWindow() {
  const [flags, setFlags] = React.useState<FeatureFlags>(featureFlags.getFlags());

  const handleFlagChange = (key: keyof FeatureFlags, value: any) => {
    featureFlags.updateFlag(key, value);
    setFlags({ ...featureFlags.getFlags() });
  };

  return (
    <div className="h-screen bg-white dark:bg-gray-900 text-black dark:text-white">
      <div className="p-4">
        <h1 className="text-xl font-bold mb-4">Feature Flags</h1>
        <div className="space-y-4">
          {Object.entries(flags).map(([key, value]) => (
            <div key={key} className="flex flex-col">
              <label className="text-sm font-medium mb-1">{key}</label>
              <input
                type={typeof value === 'string' ? 'text' : 'checkbox'}
                value={typeof value === 'string' ? value : undefined}
                checked={typeof value === 'boolean' ? value : undefined}
                onChange={(e) => {
                  const newValue = typeof value === 'boolean' ? e.target.checked : e.target.value;
                  handleFlagChange(key as keyof FeatureFlags, newValue);
                }}
                className="border rounded p-2 dark:bg-gray-800 dark:border-gray-700"
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}