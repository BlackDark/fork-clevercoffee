import { useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Label } from "@/components/ui/label";
import {
  Download,
  Upload,
  Loader2,
  HardDrive,
  Link2,
  CheckCircle,
  XCircle,
} from "lucide-react";
import { toast } from "sonner";
import { apiFetch } from "@/lib/api-config";

export function OTAUpdateSection() {
  // Local state for OTA operations
  const [otaProgress, setOtaProgress] = useState(0);
  const [otaStatus, setOtaStatus] = useState("idle");
  const [otaMessage, setOtaMessage] = useState("");
  const [otaUpdateType, setOtaUpdateType] = useState<"firmware" | "filesystem">(
    "firmware"
  );
  const [selectedOtaFile, setSelectedOtaFile] = useState<File | null>(null);
  const [otaUrl, setOtaUrl] = useState("");
  const [isOtaUploading, setIsOtaUploading] = useState(false);
  const [isUrlUpdating, setIsUrlUpdating] = useState(false);

  // Helper to format file size
  const formatFileSize = (bytes: number) => {
    if (bytes === 0) return "0 Bytes";
    const k = 1024;
    const sizes = ["Bytes", "KB", "MB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  };

  // Helper to reset OTA status
  const resetOtaStatus = () => {
    setOtaProgress(0);
    setOtaStatus("idle");
    setOtaMessage("");
  };

  // Helper to poll OTA status from backend
  const pollOtaStatus = async () => {
    try {
      const response = await fetch("/api/ota/status");
      if (!response.ok) return null;
      const status = await response.json();
      setOtaProgress(status.progress ?? 0);
      setOtaStatus(status.status ?? "idle");
      setOtaMessage(status.error || status.message || "");
      return status;
    } catch {
      return null;
    }
  };

  // File selection handler
  const handleOtaFileSelect = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files ? event.target.files[0] : null;
    setSelectedOtaFile(file);

    if (file) {
      // Validate file based on update type
      let isValidFile = false;
      let maxSize = 0;
      let fileTypeDescription = "";

      if (otaUpdateType === "firmware") {
        isValidFile = file.name.toLowerCase().endsWith(".bin");
        maxSize = 10 * 1024 * 1024; // 10MB
        fileTypeDescription = ".bin firmware file";
      } else if (otaUpdateType === "filesystem") {
        const validExtensions = [".bin", ".img"];
        isValidFile = validExtensions.some((ext) =>
          file.name.toLowerCase().endsWith(ext)
        );
        maxSize = 5 * 1024 * 1024; // 5MB
        fileTypeDescription = ".bin or .img filesystem file";
      }

      if (!isValidFile) {
        toast.error("Invalid file type", {
          description: `Please select a valid ${fileTypeDescription}.`,
        });
        setSelectedOtaFile(null);
        return;
      }

      if (file.size > maxSize) {
        toast.error("File too large", {
          description: `${
            otaUpdateType.charAt(0).toUpperCase() + otaUpdateType.slice(1)
          } file is too large. Maximum size is ${Math.floor(
            maxSize / 1024 / 1024
          )}MB.`,
        });
        setSelectedOtaFile(null);
        return;
      }

      toast.success(
        `${
          otaUpdateType.charAt(0).toUpperCase() + otaUpdateType.slice(1)
        } file selected`,
        {
          description: `Selected: ${file.name} (${formatFileSize(file.size)})`,
        }
      );
    }
  };

  // File upload handler
  const uploadOtaFile = async () => {
    if (!selectedOtaFile) {
      toast.error("No file selected", {
        description: `Please select a ${otaUpdateType} file first.`,
      });
      return;
    }

    toast(
      `Confirm ${
        otaUpdateType.charAt(0).toUpperCase() + otaUpdateType.slice(1)
      } Update`,
      {
        description: `This will update the device ${otaUpdateType}. The device will restart automatically. Are you sure?`,
        action: {
          label: `Update ${
            otaUpdateType.charAt(0).toUpperCase() + otaUpdateType.slice(1)
          }`,
          onClick: async () => {
            // Double-check if an update is already in progress
            const currentStatus = await pollOtaStatus();
            if (currentStatus && currentStatus.updateInProgress) {
              toast.warning("Update already in progress", {
                description: "Please wait for the current update to complete.",
              });
              return;
            }

            resetOtaStatus();
            setIsOtaUploading(true);

            // Start polling for progress
            const pollInterval = setInterval(async () => {
              const status = await pollOtaStatus();
              if (
                status &&
                (status.status === "complete" || status.status === "error")
              ) {
                clearInterval(pollInterval);
              }
            }, 1000);

            try {
              const formData = new FormData();
              formData.append(otaUpdateType, selectedOtaFile);

              const response = await apiFetch(`/ota/${otaUpdateType}`, {
                method: "POST",
                body: formData,
              });

              const result = await response.json();

              if (result.success) {
                toast.success(
                  `${
                    otaUpdateType.charAt(0).toUpperCase() +
                    otaUpdateType.slice(1)
                  } update successful`,
                  {
                    description: `Device will restart with new ${otaUpdateType}...`,
                  }
                );
                setSelectedOtaFile(null);
              } else {
                const errorMessage = result.message || "Unknown error occurred";

                if (response.status === 409) {
                  toast.warning("Update already in progress", {
                    description: errorMessage,
                  });
                } else {
                  toast.error(
                    `${
                      otaUpdateType.charAt(0).toUpperCase() +
                      otaUpdateType.slice(1)
                    } update failed`,
                    {
                      description: errorMessage,
                    }
                  );
                }
              }
            } catch (error: unknown) {
              console.error(`${otaUpdateType} upload error:`, error);
              toast.error(
                `${
                  otaUpdateType.charAt(0).toUpperCase() + otaUpdateType.slice(1)
                } update failed`,
                {
                  description: "Network error or device restarting...",
                }
              );
            } finally {
              clearInterval(pollInterval);
              setIsOtaUploading(false);
              // Reset status after a delay (only if not successful)
              if (otaStatus !== "complete") {
                setTimeout(() => {
                  resetOtaStatus();
                }, 5000);
              }
            }
          },
        },
      }
    );
  };

  // URL update handler
  const updateFromUrl = async () => {
    if (!otaUrl.trim()) {
      toast.error("No URL provided", {
        description: "Please enter a firmware download URL.",
      });
      return;
    }

    toast("Confirm URL Update", {
      description:
        "This will download and install firmware from the provided URL. The device will restart automatically. Are you sure?",
      action: {
        label: "Update from URL",
        onClick: async () => {
          // Double-check if an update is already in progress
          const currentStatus = await pollOtaStatus();
          if (currentStatus && currentStatus.updateInProgress) {
            toast.warning("Update already in progress", {
              description: "Please wait for the current update to complete.",
            });
            return;
          }

          resetOtaStatus();
          setIsUrlUpdating(true);

          // Start polling for progress
          const pollInterval = setInterval(async () => {
            const status = await pollOtaStatus();
            if (
              status &&
              (status.status === "complete" || status.status === "error")
            ) {
              clearInterval(pollInterval);
            }
          }, 1000);

          try {
            const response = await apiFetch("/ota/url", {
              method: "POST",
              headers: {
                "Content-Type": "application/x-www-form-urlencoded",
              },
              body: `url=${encodeURIComponent(otaUrl)}`,
            });

            const result = await response.json();

            if (result.success) {
              toast.success("Firmware update successful", {
                description: "Device will restart with new firmware...",
              });
              setOtaUrl("");
            } else {
              const errorMessage = result.message || "Unknown error occurred";

              if (response.status === 409) {
                toast.warning("Update already in progress", {
                  description: errorMessage,
                });
              } else {
                toast.error("Firmware update failed", {
                  description: errorMessage,
                });
              }
            }
          } catch (error: unknown) {
            console.error("URL update error:", error);
            toast.error("Firmware update failed", {
              description: "Network error or device restarting...",
            });
          } finally {
            clearInterval(pollInterval);
            setIsUrlUpdating(false);
            // Reset status after a delay (only if not successful)
            if (otaStatus !== "complete") {
              setTimeout(() => {
                resetOtaStatus();
              }, 5000);
            }
          }
        },
      },
    });
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3 pb-2 border-b">
        <HardDrive className="h-6 w-6 text-purple-600" />
        <h2 className="text-2xl font-semibold">OTA Updates</h2>
      </div>

      <div className="grid gap-3">
        <Label htmlFor="otaType">Select Update Type (default: Firmware)</Label>
        <Select
          value={otaUpdateType}
          onValueChange={(value: "firmware" | "filesystem") => {
            setOtaUpdateType(value);
            setSelectedOtaFile(null);
            setOtaUrl("");
          }}
        >
          <SelectTrigger>
            <SelectValue placeholder="Select update type" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="firmware">Firmware (.bin)</SelectItem>
            <SelectItem value="filesystem">Filesystem (.bin/.img)</SelectItem>
          </SelectContent>
        </Select>
      </div>

      <div className="mb-6" style={{ maxWidth: "320px" }}></div>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* OTA File Upload */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-purple-500/10">
                <HardDrive className="h-5 w-5 text-purple-600" />
              </div>
              OTA Update (File)
              <span className="ml-2 px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-700 border border-gray-300">
                {otaUpdateType === "firmware" ? "Firmware" : "Filesystem"}
              </span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-3">
              <Label htmlFor="otaFileInput">Select OTA File</Label>
              <Input
                type="file"
                id="otaFileInput"
                accept={otaUpdateType === "firmware" ? ".bin" : ".bin,.img"}
                onChange={handleOtaFileSelect}
              />
            </div>
            <p className="text-muted-foreground">
              Upload a {otaUpdateType} file to update the device. The device
              will restart automatically after successful update.
              {isUrlUpdating && (
                <span className="block text-orange-600 text-sm mt-1">
                  ⚠️ URL update is in progress. Please wait for it to complete.
                </span>
              )}
            </p>

            <div className="space-y-3">
              <Button
                onClick={uploadOtaFile}
                disabled={!selectedOtaFile || isOtaUploading || isUrlUpdating}
                className="bg-purple-500 hover:bg-purple-600 text-white w-full"
              >
                {isOtaUploading ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Upload className="mr-2 h-4 w-4" />
                )}
                {isOtaUploading
                  ? "Updating..."
                  : `Upload ${
                      otaUpdateType.charAt(0).toUpperCase() +
                      otaUpdateType.slice(1)
                    }`}
              </Button>
            </div>

            {selectedOtaFile && !isOtaUploading && (
              <Alert className="border-purple-500 bg-purple-50">
                <AlertDescription>
                  Selected: {selectedOtaFile.name} (
                  {formatFileSize(selectedOtaFile.size)})
                </AlertDescription>
              </Alert>
            )}

            {/* OTA Progress Display */}
            {(isOtaUploading || otaStatus !== "idle") && (
              <div className="space-y-3">
                <Alert
                  className={`${
                    otaStatus === "error"
                      ? "border-red-500 bg-red-50"
                      : otaStatus === "complete"
                      ? "border-green-500 bg-green-50"
                      : "border-blue-500 bg-blue-50"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    {otaStatus === "error" && (
                      <XCircle className="h-4 w-4 text-red-600" />
                    )}
                    {otaStatus === "complete" && (
                      <CheckCircle className="h-4 w-4 text-green-600" />
                    )}
                    {(otaStatus === "uploading" ||
                      otaStatus === "processing") && (
                      <Loader2 className="h-4 w-4 animate-spin text-blue-600" />
                    )}
                    <AlertDescription className="flex-1">
                      {otaMessage}
                    </AlertDescription>
                  </div>
                </Alert>

                {/* Progress Bar */}
                {otaProgress > 0 && (
                  <div className="w-full bg-gray-200 rounded-full h-2">
                    <div
                      className={`h-2 rounded-full transition-all duration-300 ${
                        otaStatus === "error"
                          ? "bg-red-500"
                          : otaStatus === "complete"
                          ? "bg-green-500"
                          : "bg-blue-500"
                      }`}
                      style={{ width: `${otaProgress}%` }}
                    ></div>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>

        {/* OTA URL Update */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-3">
              <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-indigo-500/10">
                <Link2 className="h-5 w-5 text-indigo-600" />
              </div>
              OTA Update (URL)
              <span className="ml-2 px-2 py-0.5 rounded text-xs font-medium bg-gray-100 text-gray-700 border border-gray-300">
                {otaUpdateType === "firmware" ? "Firmware" : "Filesystem"}
              </span>
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-3">
              <Label htmlFor="otaUrlType">Firmware Download URL</Label>
              <Input
                type="url"
                id="otaUrlType"
                placeholder={
                  otaUpdateType === "firmware"
                    ? "https://example.com/firmware.bin"
                    : "https://example.com/filesystem.bin or .img"
                }
                value={otaUrl}
                onChange={(e) => setOtaUrl(e.target.value)}
              />
            </div>
            <p className="text-muted-foreground">
              Provide a URL to download and install {otaUpdateType} directly.
              The device will restart automatically after successful update.
              {isOtaUploading && (
                <span className="block text-orange-600 text-sm mt-1">
                  ⚠️ File upload is in progress. Please wait for it to complete.
                </span>
              )}
            </p>
            <div className="space-y-3">
              <Button
                onClick={updateFromUrl}
                disabled={!otaUrl.trim() || isUrlUpdating || isOtaUploading}
                className="bg-indigo-500 hover:bg-indigo-600 text-white w-full"
              >
                {isUrlUpdating ? (
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                ) : (
                  <Download className="mr-2 h-4 w-4" />
                )}
                {isUrlUpdating ? "Updating..." : `Update from URL`}
              </Button>
            </div>
            {/* URL Update Progress Display */}
            {(isUrlUpdating || (otaStatus !== "idle" && !isOtaUploading)) && (
              <div className="space-y-3">
                <Alert
                  className={`${
                    otaStatus === "error"
                      ? "border-red-500 bg-red-50"
                      : otaStatus === "complete"
                      ? "border-green-500 bg-green-50"
                      : "border-indigo-500 bg-indigo-50"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    {otaStatus === "error" && (
                      <XCircle className="h-4 w-4 text-red-600" />
                    )}
                    {otaStatus === "complete" && (
                      <CheckCircle className="h-4 w-4 text-green-600" />
                    )}
                    {(otaStatus === "downloading" ||
                      otaStatus === "processing") && (
                      <Loader2 className="h-4 w-4 animate-spin text-indigo-600" />
                    )}
                    <AlertDescription className="flex-1">
                      {otaMessage}
                    </AlertDescription>
                  </div>
                </Alert>
                {/* Progress Bar */}
                {otaProgress > 0 && (
                  <div className="w-full bg-gray-200 rounded-full h-2">
                    <div
                      className={`h-2 rounded-full transition-all duration-300 ${
                        otaStatus === "error"
                          ? "bg-red-500"
                          : otaStatus === "complete"
                          ? "bg-green-500"
                          : "bg-indigo-500"
                      }`}
                      style={{ width: `${otaProgress}%` }}
                    ></div>
                  </div>
                )}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
