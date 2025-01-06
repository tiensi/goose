import React from 'react';
import ReactMarkdown from 'react-markdown';
import { CodeBlock } from './CodeBlock';

interface InputPreviewProps {
  text: string;
  previewRef: React.RefObject<HTMLDivElement>;
}

export const InputPreview = ({ text, previewRef }: InputPreviewProps) => (
  <div 
    ref={previewRef}
    className="w-full min-h-[2.5rem] prose dark:prose-invert max-w-none text-14 cursor-default max-h-[240px] overflow-y-auto"
    style={{
      minHeight: '1rem',
      maxHeight: '240px',
    }}
  >
    <ReactMarkdown
      components={{
        code: CodeBlock
      }}
    >
      {text || 'What should goose do?'}
    </ReactMarkdown>
  </div>
);