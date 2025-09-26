"""Test data factories using Pydantic factories."""

from decimal import Decimal
from datetime import datetime, timedelta
from typing import Any, Dict, List

from pydantic_factories import ModelFactory, Use
from polyfactory.factories import DataclassFactory
from faker import Faker

from app.models.vm import (
    VMCreateRequest,
    VMDocument,
    VMResponse,
    VMStatusResponse,
    VMAvailableResponse,
    ConsoleConfigDocument,
    GCPCreateRequest,
    TensorDockCreateRequest,
    CloudProvider,
    ConsoleType,
    OperatingSystems,
    GPUTypes,
    VMStatus,
    GCPVMType,
)

fake = Faker()


class ConsoleConfigFactory(ModelFactory[ConsoleConfigDocument]):
    """Factory for console configuration documents."""

    __model__ = ConsoleConfigDocument

    console_type = Use(lambda: fake.random_element(elements=list(ConsoleType)))
    min_cpus = Use(lambda: fake.random_int(min=2, max=16))
    min_ram = Use(lambda: fake.random_int(min=4, max=64))
    min_disk = Use(lambda: fake.random_int(min=50, max=500))
    supported_instance_types = Use(
        lambda: {
            "gcp": ["n1-standard-2", "n1-standard-4"],
            "tensordock": ["RTX3060", "RTX4090"]
        }
    )


class VMAvailableResponseFactory(ModelFactory[VMAvailableResponse]):
    """Factory for VM available response objects."""

    __model__ = VMAvailableResponse

    provider = Use(lambda: fake.random_element(elements=list(CloudProvider)))
    provider_id = Use(lambda: f"{fake.slug()}-{fake.random_int(min=1, max=99)}")
    hourly_price = Use(lambda: Decimal(str(fake.random.uniform(0.10, 5.00))))
    gpu = Use(lambda: fake.random_element(elements=list(GPUTypes)))
    avail_cpus = Use(lambda: fake.random_int(min=2, max=32))
    avail_ram = Use(lambda: fake.random_int(min=4, max=128))
    avail_disk = Use(lambda: fake.random_int(min=100, max=2000))
    instance_lat = Use(lambda: fake.latitude())
    instance_long = Use(lambda: fake.longitude())
    distance_to_user = Use(lambda: fake.random.uniform(0.0, 5000.0))


class VMCreateRequestFactory(ModelFactory[VMCreateRequest]):
    """Factory for VM creation requests."""

    __model__ = VMCreateRequest

    console_type = Use(lambda: fake.random_element(elements=list(ConsoleType)))
    provider = Use(lambda: fake.random_element(elements=list(CloudProvider)))
    provider_id = Use(lambda: f"{fake.slug()}-{fake.random_int(min=1, max=99)}")
    instance_name = Use(lambda: f"gaming-vm-{fake.slug()}")
    hourly_price = Use(lambda: Decimal(str(fake.random.uniform(0.10, 5.00))))
    instance_lat = Use(lambda: fake.latitude())
    instance_long = Use(lambda: fake.longitude())
    operating_system = Use(lambda: fake.random_element(elements=list(OperatingSystems)))
    gpu = Use(lambda: fake.random_element(elements=list(GPUTypes)))
    num_cpus = Use(lambda: fake.random_int(min=2, max=16))
    num_ram = Use(lambda: fake.random_int(min=4, max=64))
    num_disk = Use(lambda: fake.random_int(min=50, max=500))
    auto_stop_timeout = Use(lambda: fake.random_int(min=1800, max=14400))


class VMDocumentFactory(ModelFactory[VMDocument]):
    """Factory for VM documents."""

    __model__ = VMDocument

    vm_id = Use(lambda: f"vm-{fake.uuid4()}")
    status = Use(lambda: fake.random_element(elements=list(VMStatus)))
    console_types = Use(lambda: [fake.random_element(elements=list(ConsoleType))])
    provider = Use(lambda: fake.random_element(elements=list(CloudProvider)))
    provider_id = Use(lambda: f"{fake.slug()}-{fake.random_int(min=1, max=99)}")
    instance_name = Use(lambda: f"gaming-vm-{fake.slug()}")
    hourly_price = Use(lambda: Decimal(str(fake.random.uniform(0.10, 5.00))))
    instance_lat = Use(lambda: fake.latitude())
    instance_long = Use(lambda: fake.longitude())
    operating_system = Use(lambda: fake.random_element(elements=list(OperatingSystems)))
    gpu = Use(lambda: fake.random_element(elements=list(GPUTypes)))
    num_cpus = Use(lambda: fake.random_int(min=2, max=16))
    num_ram = Use(lambda: fake.random_int(min=4, max=64))
    num_disk = Use(lambda: fake.random_int(min=50, max=500))
    auto_stop_timeout = Use(lambda: fake.random_int(min=1800, max=14400))
    ssh_key = Use(lambda: f"ssh-rsa {fake.sha256()} {fake.email()}")
    instance_password = Use(lambda: fake.password())
    ip_address = Use(lambda: fake.ipv4())
    user_id = Use(lambda: fake.uuid4())
    created_at = Use(lambda: fake.date_time_between(start_date="-30d", end_date="now"))
    last_activity = Use(lambda: fake.date_time_between(start_date="-7d", end_date="now") if fake.boolean() else None)


class VMResponseFactory(ModelFactory[VMResponse]):
    """Factory for VM response objects."""

    __model__ = VMResponse

    vm_id = Use(lambda: f"vm-{fake.uuid4()}")
    status = Use(lambda: fake.random_element(elements=list(VMStatus)))
    console_type = Use(lambda: fake.random_element(elements=list(ConsoleType)))
    provider = Use(lambda: fake.random_element(elements=list(CloudProvider)))
    hourly_price = Use(lambda: Decimal(str(fake.random.uniform(0.10, 5.00))))
    created_at = Use(lambda: fake.date_time_between(start_date="-30d", end_date="now"))
    instance_lat = Use(lambda: fake.latitude())
    instance_long = Use(lambda: fake.longitude())
    operating_system = Use(lambda: fake.random_element(elements=list(OperatingSystems)))
    gpu = Use(lambda: fake.random_element(elements=list(GPUTypes)))
    last_activity = Use(lambda: fake.date_time_between(start_date="-7d", end_date="now") if fake.boolean() else None)


class VMStatusResponseFactory(ModelFactory[VMStatusResponse]):
    """Factory for VM status response objects."""

    __model__ = VMStatusResponse

    vm_id = Use(lambda: f"vm-{fake.uuid4()}")
    status = Use(lambda: fake.random_element(elements=list(VMStatus)))
    ip_address = Use(lambda: fake.ipv4() if fake.boolean() else None)
    last_activity = Use(lambda: fake.date_time_between(start_date="-7d", end_date="now") if fake.boolean() else None)


class GCPCreateRequestFactory(ModelFactory[GCPCreateRequest]):
    """Factory for GCP create requests."""

    __model__ = GCPCreateRequest

    ssh_key = Use(lambda: f"ssh-rsa {fake.sha256()} {fake.email()}")
    zone = Use(lambda: fake.random_element(elements=["us-central1-a", "us-west1-b", "europe-west1-c"]))
    machine_type = Use(lambda: fake.random_element(elements=list(GCPVMType)))
    name = Use(lambda: f"gaming-vm-{fake.slug()}")
    gpu_count = Use(lambda: fake.random_int(min=0, max=2))
    gpu_type = Use(lambda: fake.random_element(elements=list(GPUTypes)))
    disk_size_gb = Use(lambda: fake.random_int(min=100, max=500))
    disk_type = Use(lambda: "pd-ssd")
    source_image = Use(lambda: "projects/ubuntu-os-cloud/global/images/family/ubuntu-2404-lts")
    external_ip = Use(lambda: True)
    preemptible = Use(lambda: fake.boolean())


class TensorDockCreateRequestFactory(ModelFactory[TensorDockCreateRequest]):
    """Factory for TensorDock create requests."""

    __model__ = TensorDockCreateRequest

    password = Use(lambda: fake.password())
    ssh_key = Use(lambda: f"ssh-rsa {fake.sha256()} {fake.email()}")
    location_id = Use(lambda: fake.random_element(elements=["us-east", "us-west", "eu-central"]))
    name = Use(lambda: f"gaming-vm-{fake.slug()}")
    gpu_count = Use(lambda: fake.random_int(min=1, max=2))
    gpu_model = Use(lambda: fake.random_element(elements=list(GPUTypes)))
    vcpu_count = Use(lambda: fake.random_int(min=2, max=16))
    ram_gb = Use(lambda: fake.random_int(min=4, max=64))
    storage_gb = Use(lambda: fake.random_int(min=100, max=500))
    image = Use(lambda: "ubuntu2404")
    portforwards = Use(lambda: [47984, 47989, 48010, 47998, 47999, 22, 443])