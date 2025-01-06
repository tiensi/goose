import { useState, useRef, useEffect } from 'react';

interface UseInputStateProps {
  handleSubmit: (e: React.FormEvent) => void;
}

export const useInputState = ({ handleSubmit }: UseInputStateProps) => {
  const [text, setText] = useState('');
  const [isPreview, setIsPreview] = useState(false);
  const [cursorPosition, setCursorPosition] = useState<number>(0);
  const textAreaRef = useRef<HTMLTextAreaElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);

  const maxHeight = 240;

  useEffect(() => {
    if (textAreaRef.current && !isPreview) {
      textAreaRef.current.focus();
      textAreaRef.current.setSelectionRange(cursorPosition, cursorPosition);
    }
  }, [isPreview, cursorPosition]);

  useEffect(() => {
    const textarea = textAreaRef.current;
    if (textarea && !isPreview) {
      textarea.style.height = "0px";
      const scrollHeight = textarea.scrollHeight;
      textarea.style.height = Math.min(scrollHeight, maxHeight) + "px";
    }
  }, [text, isPreview]);

  const handleChange = (evt: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = evt.target?.value;
    setText(val);
    setCursorPosition(evt.target.selectionStart);
  };

  const handleKeyDown = (evt: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (evt.key === 'Enter' && !evt.shiftKey) {
      evt.preventDefault();
      if (text.trim()) {
        handleSubmit(new CustomEvent('submit', { detail: { value: text } }));
        setText('');
        setCursorPosition(0);
      }
    }
  };

  const onFormSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (text.trim()) {
      handleSubmit(new CustomEvent('submit', { detail: { value: text } }));
      setText('');
      setCursorPosition(0);
      setIsPreview(false);
    }
  };

  const handleFileSelect = async () => {
    const path = await window.electron.selectFileOrDirectory();
    if (path) {
      setText(path);
      setCursorPosition(path.length);
      textAreaRef.current?.focus();
    }
  };

  const handleFormat = (type: string) => {
    const textarea = textAreaRef.current;
    if (!textarea) return;

    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    const selectedText = text.substring(start, end);
    
    let newText = text;
    let newCursorPos = start;

    switch (type) {
      case 'bold':
        newText = text.substring(0, start) + `**${selectedText}**` + text.substring(end);
        newCursorPos = start + 2 + selectedText.length;
        break;
      case 'italic':
        newText = text.substring(0, start) + `*${selectedText}*` + text.substring(end);
        newCursorPos = start + 1 + selectedText.length;
        break;
      case 'ul':
        newText = text.substring(0, start) + `- ${selectedText}` + text.substring(end);
        newCursorPos = start + 2 + selectedText.length;
        break;
      case 'ol':
        newText = text.substring(0, start) + `1. ${selectedText}` + text.substring(end);
        newCursorPos = start + 3 + selectedText.length;
        break;
      case 'quote':
        newText = text.substring(0, start) + `> ${selectedText}` + text.substring(end);
        newCursorPos = start + 2 + selectedText.length;
        break;
    }

    setText(newText);
    setCursorPosition(newCursorPos);
    
    setTimeout(() => {
      textarea.focus();
      textarea.setSelectionRange(newCursorPos, newCursorPos);
    }, 0);
  };

  const isCodeBlock = (text: string): { isBlock: boolean; content: string; startIndex: number; endIndex: number } => {
    const codeBlockRegex = /```(\w*)\n([\s\S]*?)\n```/;
    const match = text.match(codeBlockRegex);
    
    if (match) {
      return {
        isBlock: true,
        content: match[2],
        startIndex: match.index || 0,
        endIndex: (match.index || 0) + match[0].length
      };
    }
    
    return { isBlock: false, content: '', startIndex: -1, endIndex: -1 };
  };

  const formatCodeBlock = (code: string, ensureNewlines = true) => {
    const lines = code.split('\n').map(line => line.trimEnd());
    while (lines.length > 0 && lines[0].trim() === '') lines.shift();
    while (lines.length > 0 && lines[lines.length - 1].trim() === '') lines.pop();
    
    return lines.length === 1 && !ensureNewlines ? lines[0] : lines.join('\n');
  };

  const handleCodeFormat = () => {
    setIsPreview(false);
    const textarea = textAreaRef.current;
    if (!textarea) return;

    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;
    
    if (start === end) {
      // No text selected - check if cursor is within a code block
      const fullText = text;
      
      for (let i = start; i >= 0; i--) {
        const textToCheck = fullText.substring(i, fullText.length);
        const blockInfo = isCodeBlock(textToCheck);
        
        if (blockInfo.isBlock && 
            i + blockInfo.startIndex <= start && 
            i + blockInfo.endIndex >= start) {
          // Remove code block
          const newValue = 
            fullText.substring(0, i + blockInfo.startIndex) +
            formatCodeBlock(blockInfo.content, false) +
            fullText.substring(i + blockInfo.endIndex);
          
          setText(newValue);
          const newPosition = i + blockInfo.startIndex;
          setCursorPosition(newPosition);
          
          setTimeout(() => {
            textarea.focus();
            textarea.setSelectionRange(newPosition, newPosition);
          }, 0);
          return;
        }
      }
      
      // Insert new code block
      const cursorPosition = start;
      const newValue = text.substring(0, cursorPosition) + 
        '\n```\n\n```\n' + 
        text.substring(cursorPosition);
      
      setText(newValue);
      const newPosition = cursorPosition + 5;
      setCursorPosition(newPosition);
      
      setTimeout(() => {
        textarea.focus();
        textarea.setSelectionRange(newPosition, newPosition);
      }, 0);
    } else {
      // Handle selected text
      const selectedText = text.substring(start, end);
      const blockInfo = isCodeBlock(selectedText);
      
      if (blockInfo.isBlock) {
        // Remove code block formatting
        const content = formatCodeBlock(blockInfo.content, false);
        const newValue = text.substring(0, start) + 
          content + 
          text.substring(end);
        
        setText(newValue);
        setCursorPosition(start + content.length);
        
        setTimeout(() => {
          textarea.focus();
          textarea.setSelectionRange(start, start + content.length);
        }, 0);
      } else {
        // Add code block formatting
        const formattedCode = formatCodeBlock(selectedText);
        const newValue = text.substring(0, start) + 
          '```\n' + formattedCode + '\n```' + 
          text.substring(end);
        
        setText(newValue);
        const newPosition = start + 5 + formattedCode.length;
        setCursorPosition(newPosition);
        
        setTimeout(() => {
          textarea.focus();
          textarea.setSelectionRange(newPosition, newPosition);
        }, 0);
      }
    }
  };

  const togglePreview = () => {
    if (!isPreview && textAreaRef.current) {
      setCursorPosition(textAreaRef.current.selectionStart);
    }
    setIsPreview(!isPreview);
  };

  return {
    text,
    setText,
    isPreview,
    setIsPreview,
    cursorPosition,
    setCursorPosition,
    textAreaRef,
    previewRef,
    handleChange,
    handleKeyDown,
    onFormSubmit,
    handleFileSelect,
    handleFormat,
    handleCodeFormat,
    togglePreview
  };
};