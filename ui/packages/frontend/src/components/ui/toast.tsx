import * as React from "react";
import { cn } from "@/lib/utils";
import { X } from "lucide-react";

export interface ToastProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?: "default" | "destructive" | "success";
}

const Toast = React.forwardRef<HTMLDivElement, ToastProps>(
  ({ className, variant = "default", children, ...props }, ref) => {
    return (
      <div
        ref={ref}
        className={cn(
          "group pointer-events-auto relative flex w-full items-center justify-between space-x-4 overflow-hidden rounded-md border p-6 pr-8 shadow-lg transition-all",
          {
            "border-border bg-background text-foreground":
              variant === "default",
            "destructive group border-destructive bg-destructive text-destructive-foreground":
              variant === "destructive",
            "border-green-200 bg-green-50 text-green-800 dark:border-green-800 dark:bg-green-950 dark:text-green-200":
              variant === "success",
          },
          className
        )}
        {...props}
      >
        {children}
      </div>
    );
  }
);
Toast.displayName = "Toast";

export interface ToastActionElement {
  altText: string;
  action: () => void;
  label: string;
}

const ToastAction = React.forwardRef<
  HTMLButtonElement,
  React.ButtonHTMLAttributes<HTMLButtonElement> & ToastActionElement
>(({ className, altText, action, label, ...props }, ref) => (
  <button
    ref={ref}
    className={cn(
      "inline-flex h-8 shrink-0 items-center justify-center rounded-md border bg-transparent px-3 text-sm font-medium ring-offset-background transition-colors hover:bg-secondary focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:pointer-events-none disabled:opacity-50",
      className
    )}
    onClick={action}
    aria-label={altText}
    {...props}
  >
    {label}
  </button>
));
ToastAction.displayName = "ToastAction";

const ToastClose = React.forwardRef<
  HTMLButtonElement,
  React.ComponentPropsWithoutRef<"button">
>(({ className, ...props }, ref) => (
  <button
    ref={ref}
    className={cn(
      "absolute right-2 top-2 rounded-md p-1 text-foreground/50 opacity-0 transition-opacity hover:text-foreground focus:opacity-100 focus:outline-none focus:ring-2 group-hover:opacity-100",
      className
    )}
    {...props}
  >
    <X className="h-4 w-4" />
  </button>
));
ToastClose.displayName = "ToastClose";

const ToastTitle = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn("text-sm font-semibold", className)}
    {...props}
  />
));
ToastTitle.displayName = "ToastTitle";

const ToastDescription = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn("text-sm opacity-90", className)} {...props} />
));
ToastDescription.displayName = "ToastDescription";

export { Toast, ToastAction, ToastClose, ToastTitle, ToastDescription };
