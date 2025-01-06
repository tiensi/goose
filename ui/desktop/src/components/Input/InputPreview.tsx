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
    className="w-full min-h-[1rem] max-h-[240px] prose dark:prose-invert max-w-none text-14 cursor-default overflow-y-auto pr-3"
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