# Browser observation contract

This module describes current DOM evidence for the shared Rust engine. It does not install an extension, attach to Chrome, read credentials, perform clicks, submit requests, solve CAPTCHA, or connect to a production ticketing site. Choosing the desktop–browser transport remains separate work.

```js
import { classifyObservation } from './observer.js';

const event = classifyObservation({
  documentId: 'navigation-42', // Assigned by the eventual transport, not the URL.
  frameId: 'main',
  controls: [{
    id: 'navigation-42/book',
    role: 'button',
    accessibleName: '예매하기',
    visible: true,
    enabled: false,
  }, {
    id: 'navigation-42/countdown',
    semantic: 'countdown',
    visible: true,
    enabled: false,
  }],
}, { desired_date: '2027-01-16', desired_session: '19:00' });
// entry_observed / sale_gate / enabled:false — never an instruction to click.
```

`classifyObservation` returns the exact `Event::EntryObserved` JSON fields. `inspectObservation` also returns evidence and the reason for classification. A later bridge must acknowledge actual input separately and send these observations through the engine's event protocol.

`collectControls(document, options)` assigns stable element IDs within one document and frame. It collects current visible controls, full calendar dates from accessible labels, and session times from machine-readable `datetime` values. It never reads CAPTCHA input values. Transport code must enumerate frames, assign a fresh `documentId` on navigation, and invalidate earlier element IDs. Opening a new window and navigating the current window use the same document identity rule.

Supply verified site markers with `contract.selectors`, for example `{ captcha: '[data-fixture-captcha]', seat_map: '[data-fixture-seat-map]' }`. These are **fixture examples**, not verified NOL selectors. Map, queue, CAPTCHA, sale-gate, conflict and completion selectors belong to the platform adapter and can be discovered after the screen opens. Users are not required to register hidden future screens.

Where a calendar exposes its selected state only as a CSS class, configure `contract.selectedSelectors.date` (and `.session` for session controls). These selectors match the control itself. Without an explicit selected state, the observer does not assume that a previous click succeeded.

CAPTCHA and queue markers take priority. Dates require a complete year, month and day; session matches use the complete time or exact accessible label. Duplicate matches and mixed document/frame observations yield `unknown`. A disabled sale button stays disabled. Conflicts or successful selection/completion require their own explicit observation protocol; absence of an error never counts as success.

The pure tests cover synthetic DOM records and fixture contracts only. No authenticated NOL booking screen or production selector has been verified by this module.
