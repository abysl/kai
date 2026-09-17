package blue.rae.kai;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.graphics.Insets;
import android.os.Build;
import android.os.Bundle;
import android.text.InputType;
import android.util.Base64;
import android.util.DisplayMetrics;
import android.view.DisplayCutout;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import android.view.inputmethod.EditorInfo;
import com.google.androidgamesdk.GameActivity;
import com.google.androidgamesdk.gametextinput.State;
import com.journeyapps.barcodescanner.ScanOptions;
import java.nio.charset.StandardCharsets;

public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("kai");
    }

    private static final int SCAN_REQUEST = 0xC0DE;
    private static final int PLAYMAT_REQUEST = 0xC0DF;

    public void choosePlaymatPicture() {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("image/*");
            try {
                startActivityForResult(intent, PLAYMAT_REQUEST);
            } catch (RuntimeException error) {
                nativePlaymatPicture(null, "Could not open the picture picker");
            }
        });
    }

    private void readPlaymatPicture(Intent data) {
        new Thread(() -> {
            try (java.io.InputStream input = getContentResolver().openInputStream(data.getData());
                 java.io.ByteArrayOutputStream output = new java.io.ByteArrayOutputStream()) {
                if (input == null) throw new java.io.IOException("Picture unavailable");
                byte[] buffer = new byte[8192];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    if (output.size() + count > 16 * 1024 * 1024) {
                        throw new java.io.IOException("Choose a picture smaller than 16 MiB");
                    }
                    output.write(buffer, 0, count);
                }
                nativePlaymatPicture(output.toByteArray(), "");
            } catch (Exception error) {
                nativePlaymatPicture(null, "Could not read picture: " + error.getMessage());
            }
        }, "kai-picture").start();
    }

    private static native void nativePlaymatPicture(byte[] bytes, String error);

    private static final String AUTOPLAY_EXTRA = "autoplay";

    private static final String AUTOPLAY_EXTRA_B64 = "autoplay_b64";

    private static final int TEXT_INPUT_TYPE =
            InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;

    private static final int TEXT_IME_OPTIONS = EditorInfo.IME_ACTION_DONE
            | EditorInfo.IME_FLAG_NO_FULLSCREEN
            | EditorInfo.IME_FLAG_NO_EXTRACT_UI;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        installAutoplay(getIntent());
        super.onCreate(savedInstanceState);
        applyTextEditorInfo();
        watchInsets();
    }

    private static void installAutoplay(Intent intent) {
        if (intent == null) {
            return;
        }
        String plan = intent.getStringExtra(AUTOPLAY_EXTRA);
        if (plan == null) {
            String encoded = intent.getStringExtra(AUTOPLAY_EXTRA_B64);
            if (encoded == null) {
                return;
            }
            try {
                plan = new String(Base64.decode(encoded, Base64.DEFAULT), StandardCharsets.UTF_8);
            } catch (IllegalArgumentException undecodable) {
                plan = "";
            }
        }
        nativeAutoplay(plan);
    }

    private void watchInsets() {
        View decorView = getWindow().getDecorView();
        decorView.setOnApplyWindowInsetsListener((view, insets) -> {
            publishInsets(insets);
            return view.onApplyWindowInsets(insets);
        });
    }

    private void publishInsets(WindowInsets insets) {
        float density = getResources().getDisplayMetrics().density;
        if (density <= 0f) {
            density = DisplayMetrics.DENSITY_DEFAULT / 160f;
        }
        int top = 0;
        int right = 0;
        int bottom = 0;
        int left = 0;
        int ime = 0;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Insets safe = insets.getInsets(
                    WindowInsets.Type.displayCutout() | WindowInsets.Type.systemGestures());
            top = safe.top;
            right = safe.right;
            bottom = safe.bottom;
            left = safe.left;
            ime = insets.getInsets(WindowInsets.Type.ime()).bottom;
        } else {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                Insets gestures = insets.getSystemGestureInsets();
                top = gestures.top;
                right = gestures.right;
                bottom = gestures.bottom;
                left = gestures.left;
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                DisplayCutout cutout = insets.getDisplayCutout();
                if (cutout != null) {
                    top = Math.max(top, cutout.getSafeInsetTop());
                    right = Math.max(right, cutout.getSafeInsetRight());
                    bottom = Math.max(bottom, cutout.getSafeInsetBottom());
                    left = Math.max(left, cutout.getSafeInsetLeft());
                }
            }
        }
        nativeInsets(top / density, right / density, bottom / density, left / density, ime / density);
    }

    private void applyTextEditorInfo() {
        try {
            setImeEditorInfoFields(TEXT_INPUT_TYPE, EditorInfo.IME_ACTION_DONE, TEXT_IME_OPTIONS);
        } catch (Exception unavailable) {
        }
    }

    public void setTextInputBuffer(String text) {
        runOnUiThread(() -> {
            applyTextEditorInfo();
            setTextInputState(new State(text, text.length(), text.length(), -1, -1));
        });
    }

    @Override
    public void stateChanged(State state, boolean dismissed) {
        super.stateChanged(state, dismissed);
        nativeTextInput(state == null || state.text == null ? "" : state.text, dismissed);
    }

    @Override
    public void onEditorAction(int action) {
        super.onEditorAction(action);
        nativeEditorAction(action);
    }

    public void setKeepAwake(boolean on) {
        runOnUiThread(() -> {
            if (on) {
                getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            } else {
                getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            }
        });
    }

    public void setClipboardText(String text) {
        runOnUiThread(() -> {
            ClipboardManager clipboard =
                    (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            if (clipboard != null) {
                clipboard.setPrimaryClip(ClipData.newPlainText("kai", text));
            }
        });
    }

    public void requestClipboardPaste() {
        runOnUiThread(() -> {
            String text = "";
            String failure = "";
            try {
                ClipboardManager clipboard =
                        (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                ClipData clip = clipboard == null ? null : clipboard.getPrimaryClip();
                if (clip != null && clip.getItemCount() > 0) {
                    CharSequence item = clip.getItemAt(0).coerceToText(this);
                    if (item != null) {
                        text = item.toString();
                    }
                }
            } catch (Exception unavailable) {
                failure = String.valueOf(unavailable.getMessage());
            }
            nativeClipboard(text, failure);
        });
    }

    public void startSpiritScan() {
        runOnUiThread(() -> {
            ScanOptions options = new ScanOptions()
                    .setDesiredBarcodeFormats(ScanOptions.QR_CODE)
                    .setPrompt("scan another player's identity QR")
                    .setBeepEnabled(false);
            startActivityForResult(options.createScanIntent(this), SCAN_REQUEST);
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PLAYMAT_REQUEST) {
            if (resultCode == RESULT_OK && data != null && data.getData() != null) {
                readPlaymatPicture(data);
            } else {
                nativePlaymatPicture(null, "");
            }
            return;
        }
        if (requestCode != SCAN_REQUEST || resultCode != RESULT_OK || data == null) {
            return;
        }
        String ticket = data.getStringExtra("SCAN_RESULT");
        if (ticket != null && !ticket.isEmpty()) {
            nativeTicket(ticket);
        }
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    private void hideSystemBars() {
        Window window = getWindow();
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false);
            WindowInsetsController controller = window.getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            window.getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_FULLSCREEN);
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            WindowManager.LayoutParams params = window.getAttributes();
            params.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            window.setAttributes(params);
        }
        window.getDecorView().requestApplyInsets();
    }

    private static native void nativeAutoplay(String plan);

    private static native void nativeTicket(String ticket);

    private static native void nativeClipboard(String text, String failure);

    private static native void nativeTextInput(String text, boolean dismissed);

    private static native void nativeEditorAction(int action);

    private static native void nativeInsets(float top, float right, float bottom, float left, float ime);
}
