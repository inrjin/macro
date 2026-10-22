# NOL CAPTCHA heading mask

`nol-captcha-title.mask` contains only a 473 × 39 binary mask of the fixed UI
heading **화면의 문자를 입력해주세요**, extracted from the user-provided NOL
screenshot on 2026-10-21. Pixels are row-major, most significant bit first;
the final byte is padded with zero bits. Dark heading pixels use a value of one.

The asset contains no challenge image, response, input field, button, account,
URL, event details, or full screenshot. It matches a fixed heading; it neither
recognizes nor solves CAPTCHA text.

The detector also requires the heading's white modal boundary and dark surround.
Its scope is this NOL layout. Different fonts, zoom, redesigns, and screenshots
from other platforms can fail to match. Disappearance does not prove that the
server accepted the response.
