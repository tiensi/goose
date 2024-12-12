interface FeatureFlags {
    whatCanGooseDoText: string;
    // Add more feature flags here as needed
}

class FeatureFlagsManager {
    private static instance: FeatureFlagsManager;
    private flags: FeatureFlags;

    private constructor() {
        this.flags = {
            whatCanGooseDoText: "What can goose do?",
        };

        // Make feature flags available in the developer console
        if (typeof window !== 'undefined') {
            (window as any).featureFlags = this.flags;
        }
    }

    public static getInstance(): FeatureFlagsManager {
        if (!FeatureFlagsManager.instance) {
            FeatureFlagsManager.instance = new FeatureFlagsManager();
        }
        return FeatureFlagsManager.instance;
    }

    public getFlags(): FeatureFlags {
        return this.flags;
    }

    public updateFlag<K extends keyof FeatureFlags>(key: K, value: FeatureFlags[K]): void {
        this.flags[key] = value;
    }
}

export const featureFlags = FeatureFlagsManager.getInstance();
export type { FeatureFlags };