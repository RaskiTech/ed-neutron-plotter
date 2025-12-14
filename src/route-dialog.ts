import { SearchBox, type SearchBoxOptions } from "./search";
import styles from "./route-dialog.module.css";

export interface RouteDialogOptions {
  onSuggest: (word: string) => string[];
  onRouteGenerated?: (config: RouteConfig) => void;
}

export interface RouteConfig {
  from: string;
  to: string;
  alreadySupercharged: boolean;
}

// Simple DOM creation helper
function createElement<T extends keyof HTMLElementTagNameMap>(
  tag: T,
  options: {
    className?: string;
    attributes?: Record<string, string>;
    children?: (HTMLElement | string)[];
  } = {}
): HTMLElementTagNameMap[T] {
  const element = document.createElement(tag);

  if (options.className) {
    element.className = options.className;
  }

  if (options.attributes) {
    Object.entries(options.attributes).forEach(([key, value]) => {
      element.setAttribute(key, value);
    });
  }

  if (options.children) {
    options.children.forEach(child => {
      if (typeof child === 'string') {
        element.appendChild(document.createTextNode(child));
      } else {
        element.appendChild(child);
      }
    });
  }

  return element;
}

export class RouteDialog {
  private dialog: HTMLDialogElement;
  private fromSearchBox!: SearchBox;
  private toSearchBox!: SearchBox;
  private superchargedCheckbox!: HTMLInputElement;
  private onRouteGeneratedCallback?: (config: RouteConfig) => void;

  private options: RouteDialogOptions;

  constructor(options: RouteDialogOptions) {
    this.options = options;
    this.dialog = this.createDialog();
    this.setupEventListeners();
  }

  private createDialog(): HTMLDialogElement {
    // Create dialog element
    const dialog = createElement('dialog', {
      className: styles.dialog
    });

    // Create header
    const header = createElement('div', {
      className: styles.header
    });

    const title = createElement('h2', {
      children: ['Configure Route'],
      className: styles.title
    });

    const closeBtn = createElement('button', {
      className: styles.closeBtn,
      children: ['×'],
      attributes: { type: 'button' }
    });

    header.appendChild(title);
    header.appendChild(closeBtn);

    // Create form sections
    const fromSection = this.createFormSection('From:', 'from-container');
    const toSection = this.createFormSection('To:', 'to-container');

    // Create checkbox section
    const checkboxSection = createElement('div', {
      className: styles.checkboxSection
    });

    const checkboxLabel = createElement('label', {
      className: styles.checkboxLabel
    });

    this.superchargedCheckbox = createElement('input', {
      attributes: { type: 'checkbox' },
      className: styles.checkbox
    });

    checkboxLabel.appendChild(this.superchargedCheckbox);
    checkboxLabel.appendChild(document.createTextNode('Already supercharged'));
    checkboxSection.appendChild(checkboxLabel);

    // Create buttons
    const buttonGroup = createElement('div', {
      className: styles.buttonGroup
    });

    const cancelBtn = createElement('button', {
      className: styles.cancelBtn,
      children: ['Cancel'],
      attributes: { type: 'button' }
    });

    const generateBtn = createElement('button', {
      className: styles.generateBtn,
      children: ['Generate Route'],
      attributes: { type: 'button' }
    });

    buttonGroup.appendChild(cancelBtn);
    buttonGroup.appendChild(generateBtn);

    // Assemble dialog
    dialog.appendChild(header);
    dialog.appendChild(fromSection);
    dialog.appendChild(toSection);
    dialog.appendChild(checkboxSection);
    dialog.appendChild(buttonGroup);

    // Create SearchBox instances
    this.createSearchBoxes(fromSection, toSection);

    return dialog;
  }

  private createFormSection(labelText: string, className: string): HTMLDivElement {
    const section = createElement('div', {
      className: `${styles.formSection} ${className}`
    });

    const label = createElement('label', {
      children: [labelText],
      className: styles.label
    });

    section.appendChild(label);
    return section;
  }

  private createSearchBoxes(fromSection: HTMLDivElement, toSection: HTMLDivElement): void {
    const searchBoxOptions: Omit<SearchBoxOptions, 'placeholder'> = {
      onSuggest: this.options.onSuggest,
      onClickRoute: () => { }, // Disable route button in dialog
      className: styles.dialogSearchBox
    };

    this.fromSearchBox = new SearchBox({
      ...searchBoxOptions,
      placeholder: 'Enter starting star...'
    });

    this.toSearchBox = new SearchBox({
      ...searchBoxOptions,
      placeholder: 'Enter destination star...'
    });

    this.fromSearchBox.mount(fromSection);
    this.toSearchBox.mount(toSection);
  }

  private setupEventListeners(): void {
    const closeBtn = this.dialog.querySelector(`.${styles.closeBtn}`) as HTMLButtonElement;
    const cancelBtn = this.dialog.querySelector(`.${styles.cancelBtn}`) as HTMLButtonElement;
    const generateBtn = this.dialog.querySelector(`.${styles.generateBtn}`) as HTMLButtonElement;

    // Button click handlers
    closeBtn.addEventListener('click', () => this.close());
    cancelBtn.addEventListener('click', () => this.close());
    generateBtn.addEventListener('click', () => this.handleGenerate());

    // Dialog backdrop click
    this.dialog.addEventListener('click', (event) => {
      if (event.target === this.dialog) {
        this.close();
      }
    });

    // Escape key
    this.dialog.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        this.close();
      }
    });
  }

  private handleGenerate(): void {
    const config: RouteConfig = {
      from: this.fromSearchBox.getValue().trim(),
      to: this.toSearchBox.getValue().trim(),
      alreadySupercharged: this.superchargedCheckbox.checked
    };

    if (config.from && config.to) {
      if (this.onRouteGeneratedCallback) {
        this.onRouteGeneratedCallback(config);
      }
      this.close();
    }
  }

  public open(): Promise<RouteConfig | null> {
    return new Promise((resolve) => {
      // Reset form
      this.fromSearchBox.setValue('');
      this.toSearchBox.setValue('');
      this.superchargedCheckbox.checked = false;

      // Set up one-time callback
      this.onRouteGeneratedCallback = (config: RouteConfig) => {
        resolve(config);
      };

      // Handle dialog close without generation
      const handleClose = () => {
        resolve(null);
        this.dialog.removeEventListener('close', handleClose);
      };
      this.dialog.addEventListener('close', handleClose);

      // Mount and show dialog
      if (!this.dialog.parentNode) {
        document.body.appendChild(this.dialog);
      }

      this.dialog.showModal();

      // Focus first input
      setTimeout(() => {
        this.fromSearchBox.focus();
      }, 100);
    });
  }

  public close(): void {
    this.dialog.close();
  }

  public destroy(): void {
    this.fromSearchBox.unmount();
    this.toSearchBox.unmount();
    if (this.dialog.parentNode) {
      this.dialog.parentNode.removeChild(this.dialog);
    }
  }

  public setFromValue(value: string): void {
    this.fromSearchBox.setValue(value);
  }

  public setToValue(value: string): void {
    this.toSearchBox.setValue(value);
  }

  public setSuperchargedValue(value: boolean): void {
    this.superchargedCheckbox.checked = value;
  }
}
