from fastapi import APIRouter, HTTPException, Query
from typing import Optional
from datetime import datetime, timedelta
from app.services.billing_service import BillingService
import logging

logger = logging.getLogger(__name__)

router = APIRouter()

billing_service = BillingService()

@router.get("/usage/tensordock")
async def get_tensordock_usage(
    days: int = Query(30, description="Number of days to look back", ge=1, le=365)
):
    """Get TensorDock usage and billing information"""
    
    try:
        start_date = datetime.utcnow() - timedelta(days=days)
        end_date = datetime.utcnow()
        
        usage = await billing_service.get_tensordock_usage(start_date, end_date)
        return usage
        
    except Exception as e:
        logger.error(f"Error getting TensorDock usage: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to retrieve TensorDock usage data"
        )

@router.get("/usage/gcp")
async def get_gcp_usage(
    days: int = Query(30, description="Number of days to look back", ge=1, le=365)
):
    """Get GCP usage and billing information"""
    
    try:
        start_date = datetime.utcnow() - timedelta(days=days)
        end_date = datetime.utcnow()
        
        usage = await billing_service.get_gcp_usage(start_date, end_date)
        return usage
        
    except Exception as e:
        logger.error(f"Error getting GCP usage: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to retrieve GCP usage data"
        )

@router.get("/usage/combined")
async def get_combined_usage(
    days: int = Query(30, description="Number of days to look back", ge=1, le=365)
):
    """Get combined usage report for all providers"""
    
    try:
        start_date = datetime.utcnow() - timedelta(days=days)
        end_date = datetime.utcnow()
        
        usage = await billing_service.get_combined_usage_report(start_date, end_date)
        return usage
        
    except Exception as e:
        logger.error(f"Error getting combined usage: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to retrieve combined usage data"
        )

@router.get("/usage/custom")
async def get_custom_usage(
    start_date: datetime = Query(..., description="Start date for usage report"),
    end_date: datetime = Query(..., description="End date for usage report"),
    provider: Optional[str] = Query(None, description="Provider filter: tensordock, gcp, or null for all")
):
    """Get usage report for a custom date range"""
    
    try:
        # Validate date range
        if start_date >= end_date:
            raise HTTPException(
                status_code=400,
                detail="Start date must be before end date"
            )
        
        # Limit to reasonable date ranges (1 year max)
        max_range = timedelta(days=365)
        if end_date - start_date > max_range:
            raise HTTPException(
                status_code=400,
                detail="Date range cannot exceed 365 days"
            )
        
        if provider == "tensordock":
            usage = await billing_service.get_tensordock_usage(start_date, end_date)
        elif provider == "gcp":
            usage = await billing_service.get_gcp_usage(start_date, end_date)
        else:
            usage = await billing_service.get_combined_usage_report(start_date, end_date)
        
        return usage
        
    except HTTPException:
        raise
    except Exception as e:
        logger.error(f"Error getting custom usage report: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to retrieve custom usage data"
        )

@router.get("/alerts")
async def get_billing_alerts(
    daily_limit: float = Query(50.0, description="Daily spending limit in USD", ge=0),
    monthly_limit: float = Query(500.0, description="Monthly spending limit in USD", ge=0)
):
    """Check billing alerts and limits"""
    
    try:
        alerts = await billing_service.check_billing_alerts(daily_limit, monthly_limit)
        return alerts
        
    except Exception as e:
        logger.error(f"Error getting billing alerts: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to check billing alerts"
        )

@router.get("/costs/current-month")
async def get_current_month_costs():
    """Get costs for the current month"""
    
    try:
        # Get current month start
        now = datetime.utcnow()
        month_start = now.replace(day=1, hour=0, minute=0, second=0, microsecond=0)
        
        usage = await billing_service.get_combined_usage_report(month_start, now)
        
        return {
            "month": now.strftime("%B %Y"),
            "period": {
                "start": month_start,
                "end": now
            },
            "costs": usage.get("overall_summary", {}),
            "provider_breakdown": usage.get("provider_breakdown", {})
        }
        
    except Exception as e:
        logger.error(f"Error getting current month costs: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to retrieve current month costs"
        )

@router.get("/costs/daily")
async def get_daily_costs(
    days: int = Query(7, description="Number of days to show", ge=1, le=30)
):
    """Get daily cost breakdown for the past N days"""
    
    try:
        daily_costs = []
        
        for i in range(days):
            day_start = datetime.utcnow().replace(hour=0, minute=0, second=0, microsecond=0) - timedelta(days=i)
            day_end = day_start + timedelta(hours=23, minutes=59, seconds=59)
            
            usage = await billing_service.get_combined_usage_report(day_start, day_end)
            
            daily_costs.append({
                "date": day_start.strftime("%Y-%m-%d"),
                "cost": usage.get("overall_summary", {}).get("total_estimated_cost_usd", 0),
                "vms": usage.get("overall_summary", {}).get("total_vms", 0),
                "hours": usage.get("overall_summary", {}).get("total_runtime_hours", 0)
            })
        
        # Sort by date (most recent first)
        daily_costs.reverse()
        
        return {
            "period_days": days,
            "daily_breakdown": daily_costs,
            "total_cost": sum(day["cost"] for day in daily_costs),
            "average_daily_cost": sum(day["cost"] for day in daily_costs) / days if days > 0 else 0
        }
        
    except Exception as e:
        logger.error(f"Error getting daily costs: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to retrieve daily costs"
        )