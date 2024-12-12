interface FeatureFlags {
    whatCanGooseDoText: string;
    expandedToolsByDefault: boolean;
    // Add more feature flags here as needed
}

class FeatureFlagsManager {
    private static instance: FeatureFlagsManager;
    private flags: FeatureFlags;
    private readonly STORAGE_KEY = 'goose-feature-flags';

    private constructor() {
        // Load flags from storage or use defaults
        const savedFlags = this.loadFlags();
        this.flags = {
            whatCanGooseDoText: "What can goose do?",
            expandedToolsByDefault: false,
            ...savedFlags
        };

        // Make feature flags available in the developer console
        if (typeof window !== 'undefined') {
            (window as any).featureFlags = this.flags;
        }
    }

    private loadFlags(): Partial<FeatureFlags> {
        try {
            const saved = localStorage.getItem(this.STORAGE_KEY);
            return saved ? JSON.parse(saved) : {};
        } catch {
            return {};
        }
    }

    private saveFlags(): void {
        try {
            localStorage.setItem(this.STORAGE_KEY, JSON.stringify(this.flags));
        } catch (error) {
            console.error('Failed to save feature flags:', error);
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
        this.saveFlags();
    }
}

export const featureFlags = FeatureFlagsManager.getInstance();
export type { FeatureFlags };