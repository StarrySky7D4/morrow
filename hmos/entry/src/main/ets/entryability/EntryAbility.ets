/*
 * Copyright (c) 2026 Huawei Device Co., Ltd.
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

import { AbilityConstant, ConfigurationConstant, UIAbility, Want } from '@kit.AbilityKit';
import { hilog } from '@kit.PerformanceAnalysisKit';
import { window } from '@kit.ArkUI';

const DOMAIN = 0x0000;

export default class EntryAbility extends UIAbility {
  private windowGeneration: number = 0;
  onCreate(want: Want, launchParam: AbilityConstant.LaunchParam): void {
    AppStorage.setOrCreate('hmos-foreground', true);
    AppStorage.setOrCreate('hmos-fullscreen-policy-ready', false);
    try {
      this.context.getApplicationContext().setColorMode(ConfigurationConstant.ColorMode.COLOR_MODE_NOT_SET);
    } catch (err) {
      hilog.error(DOMAIN, 'testTag', 'Failed to set colorMode. Cause: %{public}s', JSON.stringify(err));
    }
    hilog.info(DOMAIN, 'testTag', '%{public}s', 'Ability onCreate');
  }

  onDestroy(): void {
    this.windowGeneration++;
    AppStorage.setOrCreate('hmos-foreground', false);
    AppStorage.setOrCreate('hmos-fullscreen-policy-ready', false);
    hilog.info(DOMAIN, 'testTag', '%{public}s', 'Ability onDestroy');
  }

  onWindowStageCreate(windowStage: window.WindowStage): void {
    const generation = ++this.windowGeneration;
    AppStorage.setOrCreate('hmos-fullscreen-policy-ready', false);
    // Main window is created, set main page for this ability
    hilog.info(DOMAIN, 'testTag', '%{public}s', 'Ability onWindowStageCreate');

    // Keep the responsive workspace usable in both orientations while
    // respecting the user's system rotation lock.
    windowStage.getMainWindow().then(async (mainWindow: window.Window): Promise<void> => {
      if (generation !== this.windowGeneration) { return; }
      await mainWindow.setPreferredOrientation(window.Orientation.AUTO_ROTATION_RESTRICTED);
      if (generation !== this.windowGeneration) { return; }
      // This application owns its baseline bar policy. The SDK has no getter
      // for enabled bars, so fullscreen restores this explicit known policy.
      await mainWindow.setWindowSystemBarEnable(['status', 'navigation']);
      if (generation !== this.windowGeneration) { return; }
      AppStorage.setOrCreate('hmos-fullscreen-window-id', mainWindow.getWindowProperties().id);
      AppStorage.setOrCreate('hmos-fullscreen-policy-ready', true);
    }).catch((err: Error) => {
      hilog.warn(DOMAIN, 'Morrow', 'Could not establish application window policy: %{public}s', err.message);
    });

    windowStage.loadContent('pages/Index', (err) => {
      if (err.code) {
        hilog.error(DOMAIN, 'testTag', 'Failed to load the content. Cause: %{public}s', JSON.stringify(err));
        return;
      }
      hilog.info(DOMAIN, 'testTag', 'Succeeded in loading the content.');
    });
  }

  onWindowStageDestroy(): void {
    this.windowGeneration++;
    AppStorage.setOrCreate('hmos-fullscreen-policy-ready', false);
    // Main window is destroyed, release UI related resources
    hilog.info(DOMAIN, 'testTag', '%{public}s', 'Ability onWindowStageDestroy');
  }

  onForeground(): void {
    AppStorage.setOrCreate('hmos-foreground', true);
    // Ability has brought to foreground
    hilog.info(DOMAIN, 'testTag', '%{public}s', 'Ability onForeground');
  }

  onBackground(): void {
    AppStorage.setOrCreate('hmos-foreground', false);
    // Ability has back to background
    hilog.info(DOMAIN, 'testTag', '%{public}s', 'Ability onBackground');
  }
}
