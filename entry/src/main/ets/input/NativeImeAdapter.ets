import { inputMethod } from '@kit.IMEKit';
import { BusinessError } from '@kit.BasicServicesKit';

export class NativeImeState {
  active: boolean = false;
  focus: string = '';
  text: string = '';
  start: number = 0;
  end: number = 0;
  x: number = 0;
  y: number = 0;
  width: number = 1;
  height: number = 1;
  password: boolean = false;
  showRequest: number = 0;
}
class ImeInput {
  kind: string;
  focus: string;
  text: string = '';
  count: number = 0;
  start: number = 0;
  end: number = 0;
  code: number = 0;
  constructor(kind: string, focus: string) { this.kind = kind; this.focus = focus; }
}

// 焦点由宿主页面持有：输入法服务只把「已获焦的 ArkUI 编辑组件」当作输入代理。
// attach 前必须把焦点交给隐藏锚点，否则客户端会在数秒后被系统静默断开
// （日志表现为 OnAttachOptionsChanged, failed to get uv entry + 只能输入一次）。
export interface ImeFocusHost {
  focusAnchor: () => void;
  restoreEditor: () => void;
}

// Bind the system IME to the focused egui editor, without introducing an ArkUI text field.
export class NativeImeAdapter {
  private controller: inputMethod.InputMethodController = inputMethod.getController();
  private state: NativeImeState = new NativeImeState();
  private attached: string = '';
  private revision: number = 0;
  private running: boolean = false;
  private listening: boolean = false;
  private preview: string = '';
  private shownRequest: number = 0;
  private originX: number = 0;
  private originY: number = 0;
  private logged: string = '';
  private emit: (message: string) => void;
  private host: ImeFocusHost;

  constructor(emit: (message: string) => void, host: ImeFocusHost) {
    this.emit = emit;
    this.host = host;
  }
  update(state: NativeImeState): void {
    this.state = state;
    let signature: string = `${state.active}/${state.focus}/${state.showRequest}`;
    if (signature !== this.logged) {
      this.logged = signature;
      console.info(`PhotoCraft IME state active=${state.active} show=${state.showRequest} ` +
        `attached=${this.attached} focus=${state.focus}`);
    }
    this.revision++;
    this.reconcile();
  }
  setOrigin(x: number, y: number): void {
    this.originX = x; this.originY = y;
    this.revision++;
    this.reconcile();
  }
  stop(): void { this.update(new NativeImeState()); }
  private send(kind: string, text: string = '', count: number = 0, code: number = 0,
    start: number = 0, end: number = 0): void {
    // 焦点标识必须与已绑定的编辑框一致，防止迟到的输入法事件写入另一个文本框。
    if (!this.state.active || this.attached !== this.state.focus) { return; }
    let input = new ImeInput(kind, this.attached);
    input.text = text; input.count = count; input.code = code; input.start = start; input.end = end;
    this.emit(JSON.stringify(input));
  }
  private listen(): void {
    // 监听只注册一次：反复 off/on 会让输入法服务收到 callback 为空的注销请求
    // （ImsaKit: UnRegisterListener, callback is nullptr），并可能吞掉后续注册。
    if (this.listening) { return; }
    this.listening = true;
    this.controller.on('insertText', (text: string): void => {
      this.send('ime_commit', text); this.preview = '';
    });
    this.controller.on('deleteLeft', (length: number): void => this.send('ime_delete_left', '', length));
    this.controller.on('deleteRight', (length: number): void => this.send('ime_delete_right', '', length));
    this.controller.on('sendFunctionKey', (_key: inputMethod.FunctionKey): void => this.send('ime_key'));
    this.controller.on('moveCursor', (direction: inputMethod.Direction): void => this.send('ime_key', '', 0, direction));
    this.controller.on('selectByRange', (range: inputMethod.Range): void => {
      this.send('ime_select', '', 0, 0, range.start, range.end);
    });
    this.controller.on('getLeftTextOfCursor', (length: number): string => {
      return this.state.text.substring(Math.max(0, this.state.start - length), this.state.start);
    });
    this.controller.on('getRightTextOfCursor', (length: number): string => {
      return this.state.text.substring(this.state.end, this.state.end + length);
    });
    this.controller.on('getTextIndexAtCursor', (): number => this.state.end);
    this.controller.on('setPreviewText', (text: string, range: inputMethod.Range): void => {
      if (this.preview.length === 0 && range.start >= 0 && range.end >= 0) {
        this.send('ime_select', '', 0, 0, range.start, range.end);
      }
      this.preview = text; this.send('ime_preedit', text);
    });
    this.controller.on('finishTextPreview', (): void => {
      if (this.preview.length > 0) { this.send('ime_commit', this.preview); this.preview = ''; }
    });
  }
  private cursor(): inputMethod.CursorInfo {
    return { left: this.originX + this.state.x, top: this.originY + this.state.y,
      width: this.state.width, height: this.state.height };
  }
  private fail(step: string, error: Object): void {
    let failure = error as BusinessError;
    console.error(`PhotoCraft IME ${step} failed: ${failure.code} ${failure.message}`);
  }
  private async attachNow(focus: string): Promise<void> {
    // 顺序不可颠倒：先让锚点获焦（系统据此认定这是可输入位置），再 attach 绑定时注册监听。
    this.host.focusAnchor();
    let requested: number = this.state.showRequest;
    try {
      await this.controller.attach(requested > 0, {
        inputAttribute: { textInputType: this.state.password ? inputMethod.TextInputType.VISIBLE_PASSWORD : inputMethod.TextInputType.TEXT,
          enterKeyType: inputMethod.EnterKeyType.NEWLINE },
        cursorInfo: this.cursor(), selection: { start: this.state.start, end: this.state.end }
      });
      this.attached = focus;
      this.shownRequest = requested;
      this.preview = '';
      this.listen();
      console.info(`PhotoCraft IME attached show=${requested > 0}`);
    } catch (error) {
      this.fail('attach', error as Object);
    }
  }
  private async showKeyboard(): Promise<void> {
    // 断线自愈：客户端被系统断开时 showTextInput 抛 12800009，
    // 此时清掉本地绑定态让下一轮 reconcile 重新 attach。
    this.host.focusAnchor();
    try {
      await this.controller.showTextInput();
    } catch (error) {
      this.fail('show', error as Object);
      if ((error as BusinessError).code === 12800009) {
        this.attached = '';
        this.shownRequest = 0;
        this.revision++;
      }
    }
  }
  private wait(ms: number): Promise<void> {
    return new Promise<void>((resolve: () => void): void => {
      setTimeout((): void => { resolve(); }, ms);
    });
  }
  private async reconcile(): Promise<void> {
    if (this.running) { return; }
    this.running = true;
    try {
      let revision: number;
      do {
        revision = this.revision;
        let focus: string = this.state.active ? this.state.focus : '';
        if (this.attached.length > 0 && this.attached !== focus) {
          // 单帧抖动（系统面板切换、一次空输出）不立刻断开：等一拍确认仍失焦再解绑，
          // 否则键盘会被反复收掉，表现为「只能输入一次」。
          await this.wait(300);
          if (revision !== this.revision) { continue; }
          this.preview = '';
          this.attached = '';
          this.shownRequest = 0;
          try {
            await this.controller.detach();
            console.info('PhotoCraft IME detached');
          } catch (error) {
            this.fail('detach', error as Object);
          }
          // 绑定结束后把焦点还给 XComponent，硬件快捷键才能继续到达编辑器。
          this.host.restoreEditor();
        }
        if (focus.length > 0 && this.attached.length === 0) {
          await this.attachNow(focus);
          if (this.attached.length === 0 && this.state.active) {
            // 锚点尚未真正获焦时 attach 会抛 12800003：稍等重试一次。
            await this.wait(80);
            await this.attachNow(this.state.focus);
          }
        }
        if (this.state.active && this.attached === this.state.focus) {
          if (this.state.showRequest > this.shownRequest) {
            this.shownRequest = this.state.showRequest;
            await this.showKeyboard();
          }
          await this.controller.changeSelection(this.state.text, this.state.start, this.state.end);
          await this.controller.updateCursor(this.cursor());
        }
      } while (revision !== this.revision);
    } catch (error) {
      this.fail('reconcile', error as Object);
    } finally { this.running = false; }
  }
}
