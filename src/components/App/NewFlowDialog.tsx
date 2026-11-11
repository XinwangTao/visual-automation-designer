import styles from './App.module.css';
import commonStyles from '../common/common.module.css';
import { FLOW_TEMPLATES } from '../../flowTemplates';

export interface NewFlowDialogProps {
  isOpen: boolean;
  value: string;
  onChange: (value: string) => void;
  onCancel: () => void;
  onConfirm: () => void;
  /** 选中的模板 id，空串表示空白流程 */
  templateId?: string;
  onTemplateChange?: (templateId: string) => void;
}

export function NewFlowDialog({
  isOpen,
  value,
  onChange,
  onCancel,
  onConfirm,
  templateId = '',
  onTemplateChange,
}: NewFlowDialogProps) {
  if (!isOpen) {
    return null;
  }

  return (
    <div className="confirm-dialog" onClick={onCancel} role="dialog" aria-modal="true">
      <div className={commonStyles.confirmDialogContent} onClick={(e) => e.stopPropagation()}>
        <div className={commonStyles.confirmDialogHeader}>
          <h3 className={commonStyles.confirmDialogTitle}>新建流程</h3>
        </div>
        <div className="app__dialog-body">
          <label className="app__dialog-label" htmlFor="new-flow-name">
            流程名称
          </label>
          <input
            id="new-flow-name"
            className={styles.appDialogInput}
            value={value}
            onChange={(e) => onChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                onConfirm();
              }
            }}
            autoFocus
          />

          {onTemplateChange && (
            <div className={styles.appTemplateSection}>
              <div className="app__dialog-label">起始模板</div>
              <div className={styles.appTemplateOptions}>
                <button
                  type="button"
                  className={`${styles.appTemplateOption} ${templateId === '' ? styles.appTemplateOptionActive : ''}`}
                  onClick={() => onTemplateChange('')}
                  data-testid="template-blank"
                >
                  <span className={styles.appTemplateName}>空白流程</span>
                  <span className={styles.appTemplateDesc}>从一张空画布开始</span>
                </button>
                {FLOW_TEMPLATES.map((template) => (
                  <button
                    key={template.id}
                    type="button"
                    className={`${styles.appTemplateOption} ${templateId === template.id ? styles.appTemplateOptionActive : ''}`}
                    onClick={() => onTemplateChange(template.id)}
                    data-testid={`template-${template.id}`}
                  >
                    <span className={styles.appTemplateName}>{template.name}</span>
                    <span className={styles.appTemplateDesc}>{template.description}</span>
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>
        <div className={commonStyles.confirmDialogActions}>
          <button className={commonStyles.confirmDialogBtn} onClick={onCancel} type="button">
            取消
          </button>
          <button className="confirm-dialog__btn confirm-dialog__btn--info" onClick={onConfirm} type="button">
            创建
          </button>
        </div>
      </div>
    </div>
  );
}

export default NewFlowDialog;
