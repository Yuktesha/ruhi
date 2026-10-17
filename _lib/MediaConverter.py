import tkinter as tk
from tkinter import ttk, filedialog, messagebox
import os
import sys
import threading
import time
import shutil
import hashlib

# Try to import UniversalUI
try:
    from UniversalUI import UniversalApp, UniversalSettingsDialog
except ImportError:
    # Fallback or path adjustment if needed
    sys.path.append(os.path.dirname(os.path.abspath(__file__)))
    from UniversalUI import UniversalApp, UniversalSettingsDialog

# Dependencies
try:
    from PIL import Image
except ImportError:
    Image = None

try:
    import fitz # PyMuPDF
except ImportError:
    fitz = None

class ConverterEngine:
    """
    Shared Logic for Converting Media.
    Can be used by MediaConverter_Pro or imported by QuikMotion_Pro.
    """
    def __init__(self):
        pass

    def is_supported(self, file_path):
        """Checks if file needs conversion (e.g. TIFF, HEIC, PDF)."""
        ext = os.path.splitext(file_path)[1].lower()
        return ext in ('.tif', '.tiff', '.heic', '.pdf')

    def get_target_path(self, source_path, output_dir):
        """Calculates the destination path based on optimization rules."""
        base = os.path.basename(source_path)
        name, ext = os.path.splitext(base)
        ext = ext.lower()
        
        target_ext = ext
        if ext in ('.tif', '.tiff', '.heic'):
            target_ext = ".png"
        elif ext == '.pdf':
            target_ext = ".svg"
            
        return os.path.join(output_dir, name + target_ext)

    def convert_file(self, source, target):
        """
        Converts a single file.
        Returns: (success, message)
        """
        ext = os.path.splitext(source)[1].lower()
        
        try:
            # Ensure target dir exists
            os.makedirs(os.path.dirname(target), exist_ok=True)
            
            if ext in ('.tif', '.tiff', '.heic'):
                return self._convert_image(source, target)
            elif ext == '.pdf':
                return self._convert_pdf(source, target)
            else:
                return False, "Unsupported format"
                
        except Exception as e:
            return False, str(e)

    def _convert_image(self, source, target):
        if not Image:
            return False, "Pillow (PIL) not installed."
        try:
            with Image.open(source) as img:
                img.save(target, "PNG")
            return True, "Converted to PNG"
        except Exception as e:
            return False, str(e)

    def _convert_pdf(self, source, target):
        if not fitz:
            return False, "PyMuPDF (fitz) not installed."
        try:
            doc = fitz.open(source)
            if len(doc) > 0:
                page = doc[0] # Convert first page
                svg = page.get_svg_image()
                with open(target, 'w', encoding='utf-8') as f:
                    f.write(svg)
                return True, "Converted to SVG"
            return False, "Empty PDF"
        except Exception as e:
            return False, str(e)

class MediaConverterApp(UniversalApp):
    def __init__(self, root):
        super().__init__(root, "Media Converter", "media_converter_v1", {
            "last_output_dir": "",
            "overwrite_mode": "ask" # ask, overwrite, skip
        })
        
        self.engine = ConverterEngine()
        self.files = []
        
        self.build_ui()
        
    def build_ui(self):
        # Layout: Top (Toolbar), Middle (List), Bottom (Status/Action)
        
        # Toolbar
        toolbar = ttk.Frame(self.root, padding=5)
        toolbar.pack(fill=tk.X)
        
        ttk.Button(toolbar, text="Add Files...", command=self.add_files).pack(side=tk.LEFT, padx=5)
        ttk.Button(toolbar, text="Add Folder...", command=self.add_folder).pack(side=tk.LEFT, padx=5)
        ttk.Button(toolbar, text="Clear List", command=self.clear_list).pack(side=tk.LEFT, padx=5)
        
        ttk.Button(toolbar, text="Settings", command=self.open_settings).pack(side=tk.RIGHT, padx=5)
        
        # Main List
        self.tree = ttk.Treeview(self.root, columns=("Path", "Status"), show="headings")
        self.tree.heading("Path", text="Source File")
        self.tree.heading("Status", text="Status")
        self.tree.column("Path", width=400)
        self.tree.column("Status", width=150)
        self.tree.pack(fill=tk.BOTH, expand=True, padx=10, pady=5)
        
        # Output Config
        config_frame = ttk.LabelFrame(self.root, text="Output Options", padding=10)
        config_frame.pack(fill=tk.X, padx=10, pady=5)
        
        # Mode Selection
        ttk.Label(config_frame, text="Output Folder:").pack(side=tk.LEFT)
        self.output_var = tk.StringVar(value="Source Folder/converted")
        entry = ttk.Entry(config_frame, textvariable=self.output_var, width=50)
        entry.pack(side=tk.LEFT, padx=5)
        
        ttk.Button(config_frame, text="Browse...", command=self.browse_output).pack(side=tk.LEFT)
        
        # Action
        action_frame = ttk.Frame(self.root, padding=10)
        action_frame.pack(fill=tk.X)
        
        self.progress = ttk.Progressbar(action_frame, orient=tk.HORIZONTAL, mode='determinate')
        self.progress.pack(fill=tk.X, pady=5)
        
        self.convert_btn = ttk.Button(action_frame, text="Convert All", command=self.start_conversion)
        self.convert_btn.pack(pady=5)
        
    def add_files(self):
        files = filedialog.askopenfilenames(title="Select Media", filetypes=[("Media", "*.tif *.tiff *.pdf *.heic *.jpg *.png")])
        for f in files:
            self.add_file_to_tree(f)
            
    def add_folder(self):
        d = filedialog.askdirectory()
        if d:
            for root, _, files in os.walk(d):
                for f in files:
                    if self.engine.is_supported(f): # Only add supported/convertible files? Or all?
                        # Let's add supported only for now
                        self.add_file_to_tree(os.path.join(root, f))

    def add_file_to_tree(self, path):
        # Check duplicates
        for item in self.tree.get_children():
            if self.tree.item(item, "values")[0] == path:
                return
        self.files.append(path)
        self.tree.insert("", tk.END, values=(path, "Pending"))
        
    def clear_list(self):
        self.files.clear()
        self.tree.delete(*self.tree.get_children())
        
    def browse_output(self):
        d = filedialog.askdirectory()
        if d:
            self.output_var.set(d)
            
    def start_conversion(self):
        if not self.files:
            return
            
        output_template = self.output_var.get()
        overwrite_mode = self.config.get("overwrite_mode", "ask")
        
        # Lock UI
        self.convert_btn.config(state="disabled")
        self.progress["maximum"] = len(self.files)
        self.progress["value"] = 0
        
        # Start processing in a separate thread to keep UI responsive
        threading.Thread(target=self._run_conversion, args=(output_template, overwrite_mode), daemon=True).start()
        
    def _run_conversion(self, output_template, overwrite_mode):
        processed = 0
        
        # Session State for "Yes to All" / "Skip All"
        session_overwrite = None 
        
        if overwrite_mode == "overwrite":
            session_overwrite = True
        elif overwrite_mode == "skip":
            session_overwrite = False
            
        for child_id in self.tree.get_children():
            path = self.tree.item(child_id, "values")[0]
            
            # Determine Output Dir
            if output_template == "Source Folder/converted" or not output_template:
                out_dir = os.path.join(os.path.dirname(path), "converted")
            else:
                out_dir = output_template
                
            target = self.engine.get_target_path(path, out_dir)
            
            # Check Exist
            if os.path.exists(target):
                # Conflict Resolution
                should_overwrite = False
                
                if session_overwrite is not None:
                    should_overwrite = session_overwrite
                else:
                    # Ask User (Need to invoke on main thread)
                    # We use a mutable container and wait for main thread to fill it
                    response = [None]
                    
                    # Schedule dialog creation on main thread
                    self.root.after(0, lambda: self._ask_overwrite(target, response))
                    
                    # Wait for response (checking every 100ms)
                    while response[0] is None:
                        time.sleep(0.1)
                        
                    res = response[0] # yes, yes_all, no, no_all, cancel
                    
                    if res == "cancel":
                        break
                    elif res == "yes_all":
                        session_overwrite = True
                        should_overwrite = True
                    elif res == "no_all":
                        session_overwrite = False
                        should_overwrite = False
                    elif res == "yes":
                        should_overwrite = True
                    else: # no
                        should_overwrite = False
                
                if not should_overwrite:
                    self.root.after(0, lambda i=child_id: self.tree.set(i, "Status", "Skipped"))
                    processed += 1
                    self.root.after(0, lambda v=processed: self.progress.configure(value=v))
                    continue

            # Convert
            success, msg = self.engine.convert_file(path, target)
            status = "Done" if success else f"Error: {msg}"
            
            self.root.after(0, lambda i=child_id, s=status: self.tree.set(i, "Status", s))
            
            processed += 1
            self.root.after(0, lambda v=processed: self.progress.configure(value=v))
            
        self.root.after(0, lambda: self.convert_btn.config(state="normal"))
        self.root.after(0, lambda: messagebox.showinfo("Complete", "Conversion Finished!"))
        
    def _ask_overwrite(self, path, container):
        # Custom Dialog for "Yes, Yes All, No, No All"
        dialog = tk.Toplevel(self.root)
        dialog.title("File Exists")
        # Center the dialog
        dialog.geometry("400x150")
        try:
            # Attempt to center relative to root
            x = self.root.winfo_x() + (self.root.winfo_width() // 2) - 200
            y = self.root.winfo_y() + (self.root.winfo_height() // 2) - 75
            dialog.geometry(f"+{x}+{y}")
        except: pass

        dialog.transient(self.root)
        dialog.grab_set()
        
        msg = f"The file '{os.path.basename(path)}' already exists.\nOverwrite?"
        ttk.Label(dialog, text=msg, wraplength=380, anchor="center").pack(pady=10, fill=tk.X)
        
        btn_frame = ttk.Frame(dialog)
        btn_frame.pack(fill=tk.X, pady=10)
        
        def set_res(val):
            container[0] = val
            dialog.destroy()
            
        # Layout buttons nicely
        ttk.Button(btn_frame, text="Yes", width=8, command=lambda: set_res("yes")).pack(side=tk.LEFT, padx=5, expand=True)
        ttk.Button(btn_frame, text="Yes to All", width=10, command=lambda: set_res("yes_all")).pack(side=tk.LEFT, padx=5, expand=True)
        ttk.Button(btn_frame, text="Skip", width=8, command=lambda: set_res("no")).pack(side=tk.LEFT, padx=5, expand=True)
        ttk.Button(btn_frame, text="Skip All", width=10, command=lambda: set_res("no_all")).pack(side=tk.LEFT, padx=5, expand=True)
        
        # Handle close window as skip (or cancel? Cancel stops everything, skip just skips this)
        # Let's verify: user asked for Cancel option previously? 
        # "Cancel: Stop the operation."
        # So I should add a Cancel button or treat X as Cancel.
        
        dialog.protocol("WM_DELETE_WINDOW", lambda: set_res("cancel"))

if __name__ == "__main__":
    root = tk.Tk()
    app = MediaConverterApp(root)
    root.mainloop()
