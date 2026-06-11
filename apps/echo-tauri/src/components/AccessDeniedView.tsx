import { FormEvent, useEffect, useRef, useState } from "react";
import { Camera, KeyRound, LogIn, ScanQrCode, X } from "lucide-react";

interface AccessDeniedViewProps {
  onSubmitToken: (token: string) => void;
}

export default function AccessDeniedView({ onSubmitToken }: AccessDeniedViewProps) {
  const [token, setToken] = useState("");
  const [scanning, setScanning] = useState(false);

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault();
    const value = token.trim();
    if (value) onSubmitToken(value);
  };

  return (
    <div className="flex min-h-full items-center justify-center bg-bg-base px-5 py-8">
      <div className="w-full max-w-md rounded-lg border border-border-subtle bg-bg-card p-6 text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-lg bg-brand-soft text-brand">
          <KeyRound className="hidden h-6 w-6 md:block" />
          <ScanQrCode className="h-6 w-6 md:hidden" />
        </div>
        <h1 className="mt-4 text-[22px] font-semibold text-ink-primary">无权限访问</h1>
        <p className="mt-2 text-[14px] leading-relaxed text-ink-secondary">
          当前浏览器没有有效访问 token。
        </p>

        <form onSubmit={handleSubmit} className="mt-5 hidden text-left md:block">
          <div className="mt-2 flex gap-2">
            <input
              id="access-token"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              className="input-base min-w-0 flex-1 font-mono"
              placeholder="粘贴 token"
              autoComplete="off"
            />
            <button type="submit" className="btn-primary inline-flex items-center gap-2">
              <LogIn className="h-4 w-4" />
              <span>进入</span>
            </button>
          </div>
        </form>

        <div className="mt-5 rounded-lg border border-border-subtle bg-bg-panel px-4 py-5 md:hidden">
          <ScanQrCode className="mx-auto h-10 w-10 text-ink-primary" />
          <p className="mt-3 text-[14px] leading-relaxed text-ink-secondary">
            请点击扫码，扫描电脑设置页中的 token 二维码。
          </p>
          <button
            type="button"
            onClick={() => setScanning(true)}
            className="btn-primary mt-4 inline-flex items-center gap-2"
          >
            <Camera className="h-4 w-4" />
            <span>扫码获取 token</span>
          </button>
        </div>
      </div>

      {scanning && (
        <QrScanner
          onCancel={() => setScanning(false)}
          onToken={(value) => {
            setScanning(false);
            onSubmitToken(value);
          }}
        />
      )}
    </div>
  );
}

interface BarcodeDetectorConstructor {
  new (options?: { formats?: string[] }): BarcodeDetectorInstance;
  getSupportedFormats?: () => Promise<string[]>;
}

interface BarcodeDetectorInstance {
  detect(source: CanvasImageSource): Promise<Array<{ rawValue?: string }>>;
}

function QrScanner({
  onCancel,
  onToken,
}: {
  onCancel: () => void;
  onToken: (token: string) => void;
}) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let stream: MediaStream | null = null;
    let stopped = false;
    let frame = 0;

    const start = async () => {
      const Detector = (
        window as unknown as { BarcodeDetector?: BarcodeDetectorConstructor }
      ).BarcodeDetector;
      if (!Detector) {
        setError("当前浏览器不支持扫码，请使用桌面端输入 token。");
        return;
      }

      try {
        stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: "environment" },
        });
        const video = videoRef.current;
        if (!video) return;
        video.srcObject = stream;
        await video.play();

        const detector = new Detector({ formats: ["qr_code"] });
        const scan = async () => {
          if (stopped) return;
          try {
            const codes = await detector.detect(video);
            const value = codes[0]?.rawValue?.trim();
            if (value) {
              onToken(value);
              return;
            }
          } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
          }
          frame = window.setTimeout(scan, 350);
        };
        await scan();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    };

    void start();

    return () => {
      stopped = true;
      window.clearTimeout(frame);
      stream?.getTracks().forEach((track) => track.stop());
    };
  }, [onToken]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4">
      <div className="w-full max-w-sm rounded-lg border border-border-subtle bg-bg-card p-4">
        <div className="mb-3 flex items-center justify-between">
          <div className="text-[16px] font-semibold text-ink-primary">扫码获取 token</div>
          <button
            type="button"
            onClick={onCancel}
            className="flex h-8 w-8 items-center justify-center rounded text-ink-muted hover:bg-bg-hover hover:text-ink-primary"
            aria-label="关闭扫码"
          >
            <X className="h-4 w-4" />
          </button>
        </div>
        <video
          ref={videoRef}
          className="aspect-square w-full rounded-lg bg-black object-cover"
          muted
          playsInline
        />
        {error && (
          <div className="mt-3 rounded-lg border border-accent-red/30 bg-accent-red/10 px-3 py-2 text-[13px] text-accent-red">
            {error}
          </div>
        )}
      </div>
    </div>
  );
}
