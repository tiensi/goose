import React from 'react';
import { Card } from '../ui/card';
import { Lock } from 'lucide-react'
import { Input } from "../ui/input"
import { Button } from "../ui/button"
import UnionIcon from '../../images/Union@2x.svg';


interface AddModelModalProps {
  provider: string
  model: string
  endpoint: string
  onSubmit: (apiKey: string) => void
  onCancel: () => void
}

export function AddModelModal({ provider, model, endpoint, onSubmit, onCancel }: AddModelModalProps) {
    const [apiKey, setApiKey] = React.useState("")
  
    const handleSubmit = (e: React.FormEvent) => {
      e.preventDefault()
      onSubmit(apiKey)
    }
  
    return (
      <div className="fixed inset-0 bg-black/20 backdrop-blur-sm">
        <Card className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[440px] bg-white rounded-[32px] shadow-xl overflow-hidden">
          <div className="px-8 pt-12 pb-0 space-y-8">
            {/* Header */}
            <div className="text-center space-y-4">
              {/* Purple icon */}
              <div className="mx-auto w-12 h-12 flex items-center justify-center">
                  <img 
                    src={UnionIcon} 
                    alt="Union icon" 
                    className="w-32 h-32"
                  />
              </div>
              <h2 className="text-2xl font-semibold text-gray-900">Add model</h2>
              <p className="text-gray-500 text-lg">
                Add your model details to integrate into goose
              </p>
            </div>
  
            {/* Form */}
          <form onSubmit={handleSubmit} className="space-y-8">
            <div className="space-y-5">
              <div>
                <Input
                  type="text"
                  value={endpoint}
                  disabled
                  placeholder="Endpoint"
                  className="w-full h-14 px-6 rounded-2xl border border-gray-200/75 bg-white text-lg placeholder:text-gray-400"
                />
              </div>
              <div>
                <Input
                  type="text"
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder="Model"
                  className="w-full h-14 px-6 rounded-2xl border border-gray-200/75 bg-white text-lg placeholder:text-gray-400"
                  required
                />
              </div>
              <div>
                <Input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="API key"
                  className="w-full h-14 px-6 rounded-2xl border border-gray-200/75 bg-white text-lg placeholder:text-gray-400"
                  required
                />
                <div className="flex items-center gap-1.5 mt-3 text-gray-500">
                  <Lock className="w-4 h-4" />
                  <span className="text-[15px]">Keys are stored in a secure .env file</span>
                </div>
              </div>
            </div>
  
              {/* Actions */}
              <div className="-mx-8 border-t border-gray-100">
                <Button 
                  type="submit"
                  variant="ghost"
                  className="w-full h-[60px] text-gray-900 hover:bg-gray-50 rounded-none text-lg font-medium border-b border-gray-100"
                >
                  Submit
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  onClick={onCancel}
                  className="w-full h-[60px] text-gray-500 hover:bg-gray-50 rounded-none text-lg font-medium"
                >
                  Cancel
                </Button>
              </div>
            </form>
          </div>
        </Card>
      </div>
    )
  }
  
  