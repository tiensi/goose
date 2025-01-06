import React from 'react';
import { Button } from '../ui/button';
import Send from '../ui/Send';
import Stop from '../ui/Stop';
import { Paperclip, Code } from 'lucide-react';
import { PreviewButton } from './PreviewButton';

interface InputCommandBarProps {
  text: string;
  isPreview: boolean;
  disabled: boolean;
  isLoading: boolean;
  onStop?: () => void;
  togglePreview: () => void;
  handleCodeFormat: () => void;
  handleFileSelect: () => void;
  onFormSubmit: (e: React.FormEvent) => void;
}

export const InputCommandBar = ({
  text,
  isPreview,
  disabled,
  isLoading,
  onStop,
  togglePreview,
  handleCodeFormat,
  handleFileSelect,
}: InputCommandBarProps) => (
  <div className="absolute right-0 top-1/2 -translate-y-1/2 flex items-center gap-1 pl-2">
    <PreviewButton
      isPreview={isPreview}
      disabled={disabled}
      hasText={!!text.trim()}
      onClick={togglePreview}
    />
    <Button
      type="button"
      size="icon"
      variant="ghost"
      onClick={handleCodeFormat}
      disabled={disabled || isPreview}
      className={`text-indigo-600 dark:text-indigo-300 hover:text-indigo-700 dark:hover:text-indigo-200 hover:bg-indigo-100 dark:hover:bg-indigo-800 ${
        disabled || isPreview ? 'opacity-50 cursor-not-allowed' : ''
      }`}
    >
      <Code size={20} />
    </Button>
    <Button
      type="button"
      size="icon"
      variant="ghost"
      onClick={handleFileSelect}
      disabled={disabled || isPreview}
      className={`text-indigo-600 dark:text-indigo-300 hover:text-indigo-700 dark:hover:text-indigo-200 hover:bg-indigo-100 dark:hover:bg-indigo-800 ${
        disabled || isPreview ? 'opacity-50 cursor-not-allowed' : ''
      }`}
    >
      <Paperclip size={20} />
    </Button>
    {isLoading ? (
      <Button
        type="button"
        size="icon"
        variant="ghost"
        onClick={onStop}
        className="bg-indigo-100 dark:bg-indigo-800 dark:text-indigo-200 text-indigo-600 hover:opacity-50"
      >
        <Stop size={24} />
      </Button>
    ) : (
      <Button
        type="submit"
        size="icon"
        variant="ghost"
        disabled={disabled || !text.trim()}
        className={`text-indigo-600 dark:text-indigo-300 hover:text-indigo-700 dark:hover:text-indigo-200 hover:bg-indigo-100 dark:hover:bg-indigo-800 ${
          disabled || !text.trim() ? 'opacity-50 cursor-not-allowed' : ''
        }`}
      >
        <Send size={24} />
      </Button>
    )}
  </div>
);