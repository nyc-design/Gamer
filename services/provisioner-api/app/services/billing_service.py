import asyncio
import logging
from typing import Dict, Any, List, Optional
from datetime import datetime, timedelta
from app.models.vm import VMDocument, CloudProvider
from app.services.tensordock_service import TensorDockService
from app.core.config import settings
from app.core.database import get_database
import httpx

logger = logging.getLogger(__name__)

class BillingService:
    """Service for monitoring billing and usage across cloud providers"""
    
    def __init__(self):
        self.tensordock_service = TensorDockService()
        # GCP billing requires Google Cloud Billing API credentials
        self.gcp_project_id = getattr(settings, 'gcp_project_id', None)
        self.gcp_billing_account = getattr(settings, 'gcp_billing_account', None)
    
    async def get_tensordock_usage(self, start_date: datetime = None, end_date: datetime = None) -> Dict[str, Any]:
        """Get TensorDock usage and billing information"""
        
        try:
            if not start_date:
                start_date = datetime.utcnow() - timedelta(days=30)  # Last 30 days
            if not end_date:
                end_date = datetime.utcnow()
            
            # Get all TensorDock VMs from our database
            db = get_database()
            tensordock_vms_data = await db.vms.find({
                "provider": CloudProvider.TENSORDOCK,
                "created_at": {"$gte": start_date, "$lte": end_date}
            }).to_list(None)
            
            tensordock_vms = [VMDocument(**vm_data) for vm_data in tensordock_vms_data]
            
            total_cost = 0.0
            total_hours = 0.0
            vm_details = []
            
            for vm in tensordock_vms:
                try:
                    # Calculate runtime hours
                    if vm.created_at and vm.last_activity:
                        runtime = vm.last_activity - vm.created_at
                        hours = runtime.total_seconds() / 3600
                    else:
                        hours = 0
                    
                    # Estimate cost based on preset and hours
                    estimated_cost = await self._estimate_tensordock_cost(vm, hours)
                    
                    vm_details.append({
                        "vm_id": vm.vm_id,
                        "console_type": vm.console_type,
                        "preset": vm.preset,
                        "created_at": vm.created_at,
                        "last_activity": vm.last_activity,
                        "status": vm.status,
                        "runtime_hours": round(hours, 2),
                        "estimated_cost_usd": round(estimated_cost, 4)
                    })
                    
                    total_cost += estimated_cost
                    total_hours += hours
                    
                except Exception as e:
                    logger.warning(f"Error calculating cost for VM {vm.vm_id}: {str(e)}")
                    continue
            
            return {
                "provider": "tensordock",
                "period": {
                    "start": start_date,
                    "end": end_date
                },
                "summary": {
                    "total_vms": len(tensordock_vms),
                    "total_runtime_hours": round(total_hours, 2),
                    "total_estimated_cost_usd": round(total_cost, 2)
                },
                "vm_details": vm_details
            }
            
        except Exception as e:
            logger.error(f"Error getting TensorDock usage: {str(e)}")
            return {
                "provider": "tensordock",
                "error": str(e),
                "summary": {
                    "total_vms": 0,
                    "total_runtime_hours": 0,
                    "total_estimated_cost_usd": 0
                }
            }
    
    async def get_gcp_usage(self, start_date: datetime = None, end_date: datetime = None) -> Dict[str, Any]:
        """Get GCP CloudyPad usage and billing information"""
        
        try:
            if not start_date:
                start_date = datetime.utcnow() - timedelta(days=30)  # Last 30 days
            if not end_date:
                end_date = datetime.utcnow()
            
            # Get all GCP-based CloudyPad VMs
            gcp_providers = [
                CloudProvider.CLOUDYPAD_GCP
                # Add other GCP-based providers as needed
            ]
            
            db = get_database()
            gcp_vms_data = await db.vms.find({
                "provider": {"$in": gcp_providers},
                "created_at": {"$gte": start_date, "$lte": end_date}
            }).to_list(None)
            
            gcp_vms = [VMDocument(**vm_data) for vm_data in gcp_vms_data]
            
            total_cost = 0.0
            total_hours = 0.0
            vm_details = []
            
            for vm in gcp_vms:
                try:
                    # Calculate runtime hours
                    if vm.created_at and vm.last_activity:
                        runtime = vm.last_activity - vm.created_at
                        hours = runtime.total_seconds() / 3600
                    else:
                        hours = 0
                    
                    # Estimate cost based on GCP pricing
                    estimated_cost = await self._estimate_gcp_cost(vm, hours)
                    
                    vm_details.append({
                        "vm_id": vm.vm_id,
                        "console_type": vm.console_type,
                        "preset": vm.preset,
                        "provider": vm.provider,
                        "created_at": vm.created_at,
                        "last_activity": vm.last_activity,
                        "status": vm.status,
                        "runtime_hours": round(hours, 2),
                        "estimated_cost_usd": round(estimated_cost, 4)
                    })
                    
                    total_cost += estimated_cost
                    total_hours += hours
                    
                except Exception as e:
                    logger.warning(f"Error calculating GCP cost for VM {vm.vm_id}: {str(e)}")
                    continue
            
            return {
                "provider": "gcp",
                "period": {
                    "start": start_date,
                    "end": end_date
                },
                "summary": {
                    "total_vms": len(gcp_vms),
                    "total_runtime_hours": round(total_hours, 2),
                    "total_estimated_cost_usd": round(total_cost, 2)
                },
                "vm_details": vm_details
            }
            
        except Exception as e:
            logger.error(f"Error getting GCP usage: {str(e)}")
            return {
                "provider": "gcp",
                "error": str(e),
                "summary": {
                    "total_vms": 0,
                    "total_runtime_hours": 0,
                    "total_estimated_cost_usd": 0
                }
            }
    
    async def get_combined_usage_report(self, start_date: datetime = None, end_date: datetime = None) -> Dict[str, Any]:
        """Get combined usage report for all providers"""
        
        try:
            # Get usage from all providers
            tensordock_usage, gcp_usage = await asyncio.gather(
                self.get_tensordock_usage(start_date, end_date),
                self.get_gcp_usage(start_date, end_date),
                return_exceptions=True
            )
            
            # Handle exceptions
            if isinstance(tensordock_usage, Exception):
                logger.error(f"TensorDock usage error: {tensordock_usage}")
                tensordock_usage = {"provider": "tensordock", "error": str(tensordock_usage)}
            
            if isinstance(gcp_usage, Exception):
                logger.error(f"GCP usage error: {gcp_usage}")
                gcp_usage = {"provider": "gcp", "error": str(gcp_usage)}
            
            # Calculate totals
            total_cost = 0
            total_hours = 0
            total_vms = 0
            
            for usage in [tensordock_usage, gcp_usage]:
                if "summary" in usage:
                    total_cost += usage["summary"]["total_estimated_cost_usd"]
                    total_hours += usage["summary"]["total_runtime_hours"]
                    total_vms += usage["summary"]["total_vms"]
            
            return {
                "period": {
                    "start": start_date or (datetime.utcnow() - timedelta(days=30)),
                    "end": end_date or datetime.utcnow()
                },
                "overall_summary": {
                    "total_vms": total_vms,
                    "total_runtime_hours": round(total_hours, 2),
                    "total_estimated_cost_usd": round(total_cost, 2)
                },
                "provider_breakdown": {
                    "tensordock": tensordock_usage,
                    "gcp": gcp_usage
                }
            }
            
        except Exception as e:
            logger.error(f"Error generating combined usage report: {str(e)}")
            return {
                "error": str(e),
                "overall_summary": {
                    "total_vms": 0,
                    "total_runtime_hours": 0,
                    "total_estimated_cost_usd": 0
                }
            }
    
    async def _estimate_tensordock_cost(self, vm: VMDocument, hours: float) -> float:
        """Estimate TensorDock cost based on VM specs and runtime"""
        
        try:
            # TensorDock approximate pricing (USD/hour)
            # These are estimates based on typical pricing
            base_rates = {
                "retro": 0.15,     # 2 vCPU, 4GB RAM, no GPU
                "advanced": 0.35,  # 4 vCPU, 8GB RAM, GTX1060
                "premium": 1.20    # 8 vCPU, 16GB RAM, RTX4090
            }
            
            base_rate = base_rates.get(vm.preset.lower(), 0.35)
            
            # Console-specific adjustments
            console_multipliers = {
                "switch": 1.3,  # Higher GPU requirements
                "3ds": 1.1,     # Moderate GPU requirements
                "gamecube": 1.2,
                "wii": 1.2
            }
            
            multiplier = console_multipliers.get(vm.console_type.lower(), 1.0)
            final_rate = base_rate * multiplier
            
            return hours * final_rate
            
        except Exception as e:
            logger.warning(f"Error estimating TensorDock cost: {str(e)}")
            return 0.0
    
    async def _estimate_gcp_cost(self, vm: VMDocument, hours: float) -> float:
        """Estimate GCP cost based on VM specs and runtime"""
        
        try:
            # GCP Compute Engine approximate pricing (USD/hour)
            # These are estimates based on typical pricing
            base_rates = {
                "retro": 0.12,     # e2-standard-2
                "advanced": 0.28,  # n1-standard-4 + T4 GPU
                "premium": 0.85    # n1-standard-8 + T4 GPU
            }
            
            base_rate = base_rates.get(vm.preset.lower(), 0.28)
            
            # CloudyPad adds overhead for their service
            cloudypad_overhead = 1.25  # 25% markup
            
            return hours * base_rate * cloudypad_overhead
            
        except Exception as e:
            logger.warning(f"Error estimating GCP cost: {str(e)}")
            return 0.0
    
    async def check_billing_alerts(self, daily_limit: float = 50.0, monthly_limit: float = 500.0) -> Dict[str, Any]:
        """Check if billing limits are being approached or exceeded"""
        
        try:
            # Get today's usage
            today_start = datetime.utcnow().replace(hour=0, minute=0, second=0, microsecond=0)
            today_usage = await self.get_combined_usage_report(today_start)
            
            # Get this month's usage
            month_start = datetime.utcnow().replace(day=1, hour=0, minute=0, second=0, microsecond=0)
            month_usage = await self.get_combined_usage_report(month_start)
            
            daily_cost = today_usage["overall_summary"]["total_estimated_cost_usd"]
            monthly_cost = month_usage["overall_summary"]["total_estimated_cost_usd"]
            
            alerts = []
            
            # Check daily limit
            if daily_cost >= daily_limit:
                alerts.append({
                    "type": "daily_limit_exceeded",
                    "message": f"Daily spending limit exceeded: ${daily_cost:.2f} >= ${daily_limit:.2f}",
                    "severity": "critical"
                })
            elif daily_cost >= daily_limit * 0.8:
                alerts.append({
                    "type": "daily_limit_warning", 
                    "message": f"Daily spending approaching limit: ${daily_cost:.2f} (80% of ${daily_limit:.2f})",
                    "severity": "warning"
                })
            
            # Check monthly limit
            if monthly_cost >= monthly_limit:
                alerts.append({
                    "type": "monthly_limit_exceeded",
                    "message": f"Monthly spending limit exceeded: ${monthly_cost:.2f} >= ${monthly_limit:.2f}",
                    "severity": "critical"
                })
            elif monthly_cost >= monthly_limit * 0.8:
                alerts.append({
                    "type": "monthly_limit_warning",
                    "message": f"Monthly spending approaching limit: ${monthly_cost:.2f} (80% of ${monthly_limit:.2f})",
                    "severity": "warning"
                })
            
            return {
                "alerts": alerts,
                "current_usage": {
                    "daily_cost": daily_cost,
                    "monthly_cost": monthly_cost,
                    "daily_limit": daily_limit,
                    "monthly_limit": monthly_limit
                }
            }
            
        except Exception as e:
            logger.error(f"Error checking billing alerts: {str(e)}")
            return {
                "error": str(e),
                "alerts": []
            }