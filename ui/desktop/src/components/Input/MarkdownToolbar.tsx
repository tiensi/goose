import React from 'react';
import { Button } from '../ui/button';
import { Bold, Italic, ListOrdered, List, Quote } from 'lucide-react';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
} from '../ui/tooltip';

interface MarkdownToolbarProps {
  onFormat: (type: string) => void;
  disabled?: boolean;
}

export function MarkdownToolbar({ onFormat, disabled }: MarkdownToolbarProps) {
  const tools = [
    { icon: Bold, format: 'bold', tooltip: 'Bold' },
    { icon: Italic, format: 'italic', tooltip: 'Italic' },
    { icon: List, format: 'ul', tooltip: 'Bulleted List' },
    { icon: ListOrdered, format: 'ol', tooltip: 'Ordered List' },
    { icon: Quote, format: 'quote', tooltip: 'Quote' },
  ];

  return (
    <TooltipProvider>
      <div className="flex gap-1 mb-3 -ml-2">
        {tools.map((tool) => (
          <Tooltip key={tool.format}>
            <TooltipTrigger asChild>
              <Button
                type="button"
                size="icon"
                variant="ghost"
                onClick={() => onFormat(tool.format)}
                disabled={disabled}
                className="text-indigo-600 dark:text-indigo-300 hover:text-indigo-700 
                           dark:hover:text-indigo-200 hover:bg-indigo-100 dark:hover:bg-indigo-800"
              >
                <tool.icon size={16} />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>{tool.tooltip}</p>
            </TooltipContent>
          </Tooltip>
        ))}
      </div>
    </TooltipProvider>
  );
}