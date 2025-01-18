import React from 'react';
import { Search } from 'lucide-react'
import { Button } from "../../ui/button"
import { Input } from "../../ui/input"
import { ModelList } from "./ModelList"
import { ProviderButtons } from "./ProviderButtons"
import { AddModelDialog } from "./AddModelDialog"

export default function MoreModelsPage() {
    return (
        <div className="min-h-screen bg-background text-foreground">
            <div className="container max-w-6xl mx-auto p-6">
                <div className="flex items-center justify-between mb-8">
                    <h1 className="text-2xl font-semibold">More Models</h1>
                    <AddModelDialog />
                </div>

                <div className="space-y-8">
                    {/* Search section */}
                    <div className="relative">
                        <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
                        <Input
                            placeholder="Search models..."
                            className="pl-10 bg-background border-muted-foreground/20"
                        />
                    </div>

                    {/* Provider buttons */}
                    <div className="space-y-4">
                        <h2 className="text-lg font-medium">Browse by Provider</h2>
                        <ProviderButtons />
                    </div>

                    {/* Recent models */}
                    <div className="space-y-4">
                        <div className="flex items-center justify-between">
                            <h2 className="text-lg font-medium">Recently Used Models</h2>
                            <Button variant="ghost" className="text-blue-500 hover:text-blue-600">
                                View all
                            </Button>
                        </div>
                        <ModelList />
                    </div>
                </div>
            </div>
        </div>
    )
}

