from typing import Dict, List, Optional
from app.models.vm import VMResponse
import threading

class VMStore:
    """In-memory VM storage. Replace with database in production."""
    
    def __init__(self):
        self._vms: Dict[str, VMResponse] = {}
        self._lock = threading.Lock()
    
    def create_vm(self, vm: VMResponse) -> None:
        """Store a new VM record"""
        with self._lock:
            self._vms[vm.vm_id] = vm
    
    def get_vm(self, vm_id: str) -> Optional[VMResponse]:
        """Get VM by ID"""
        with self._lock:
            return self._vms.get(vm_id)
    
    def update_vm(self, vm: VMResponse) -> None:
        """Update existing VM record"""
        with self._lock:
            if vm.vm_id in self._vms:
                self._vms[vm.vm_id] = vm
    
    def delete_vm(self, vm_id: str) -> bool:
        """Delete VM record"""
        with self._lock:
            if vm_id in self._vms:
                del self._vms[vm_id]
                return True
            return False
    
    def list_vms(self) -> List[VMResponse]:
        """List all VMs"""
        with self._lock:
            return list(self._vms.values())