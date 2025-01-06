import React from 'react';
import { InputCommandBar } from './InputCommandBar';
import { InputPreview } from './InputPreview';
import { InputTextArea } from './InputTextArea';
import { useInputState } from './useInputState';
import { MarkdownToolbar } from './MarkdownToolbar';

interface InputProps {
  handleSubmit: (e: React.FormEvent) => void;
  disabled?: boolean;
  isLoading?: boolean;
  onStop?: () => void;
}

export default function Input({
  handleSubmit,
  disabled = false,
  isLoading = false,
  onStop
}: InputProps) {
  const {
    text,
    isPreview,
    textAreaRef,
    previewRef,
    handleChange,
    handleKeyDown,
    onFormSubmit,
    handleFileSelect,
    handleFormat,
    handleCodeFormat,
    togglePreview
  } = useInputState({ handleSubmit });

  return (
    <form onSubmit={onFormSubmit} className="flex relative h-auto px-[16px] py-[1rem]">
      <div className="w-full relative">
        <MarkdownToolbar onFormat={handleFormat} disabled={disabled || isPreview} />
        <div className="pr-[160px] relative">
          {isPreview ? (
            <InputPreview 
              text={text} 
              previewRef={previewRef} 
            />
          ) : (
            <InputTextArea
              text={text}
              disabled={disabled}
              textAreaRef={textAreaRef}
              handleChange={handleChange}
              handleKeyDown={handleKeyDown}
            />
          )}
        </div>
        <InputCommandBar
          text={text}
          isPreview={isPreview}
          disabled={disabled}
          isLoading={isLoading}
          onStop={onStop}
          togglePreview={togglePreview}
          handleCodeFormat={handleCodeFormat}
          handleFileSelect={handleFileSelect}
          onFormSubmit={onFormSubmit}
        />
      </div>
    </form>
  );
}