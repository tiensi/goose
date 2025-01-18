import React from 'react';
import { Clock } from 'lucide-react';

// TODO: models -- dynamically create this
const recentModels = [
    { name: "GPT-4", provider: "OpenAI", lastUsed: "2 hours ago" },
    { name: "Claude 3", provider: "Anthropic", lastUsed: "Yesterday" },
    { name: "PaLM 2", provider: "Google", lastUsed: "3 days ago" },
]

export function ModelList() {
    return (
        <div className="space-y-2">
            {recentModels.map((model) => (
                <div
                    key={model.name}
                    className="flex items-center justify-between p-4 rounded-lg border border-muted-foreground/20 bg-background hover:bg-muted/50 transition-colors"
                >
                    <div className="space-y-1">
                        <p className="font-medium">{model.name}</p>
                        <p className="text-sm text-muted-foreground">{model.provider}</p>
                    </div>
                    <div className="flex items-center text-sm text-muted-foreground">
                        <Clock className="w-4 h-4 mr-2" />
                        {model.lastUsed}
                    </div>
                </div>
            ))}
        </div>
    )
}

