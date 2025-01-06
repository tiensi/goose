import React from 'react';
import { Eye } from 'lucide-react';
import { Button } from '../ui/button';

interface PreviewButtonProps {
  isPreview: boolean;
  disabled: boolean;
  hasText: boolean;
  onClick: () => void;
}

export const PreviewButton = ({ isPreview, disabled, hasText, onClick }: PreviewButtonProps) => (
  <Button
    type="button"
    size="icon"
    variant="ghost"
    onClick={onClick}
    disabled={disabled || !hasText}
    className={`text-indigo-600 dark:text-indigo-300 hover:text-indigo-700 dark:hover:text-indigo-200 hover:bg-indigo-100 dark:hover:bg-indigo-800 ${
      isPreview ? 'bg-indigo-100 dark:bg-indigo-800' : ''
    } ${
      disabled || !hasText ? 'opacity-50 cursor-not-allowed' : ''
    }`}
  >
    <Eye size={20} />
  </Button>
);